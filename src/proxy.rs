use std::net::SocketAddr;
use anyhow::{Result, anyhow};
use tokio::net::{
    TcpListener,TcpStream,
};
use tokio_util::codec::{Decoder, Framed};
use irc_proto::{IrcCodec, Message, Command, Response};
use futures::{StreamExt, SinkExt, select};
use rasta::{schema::Room, Credentials, ServerMessage, session::Session};
use serde_json::{Value, from_value};


#[derive(Debug)]
pub struct Proxy {
    bind: String,
    backend: String,
}

#[derive(Debug)]
struct ClientInfo {
    nick: String,
    user: String,
    pass: String,
    host: String,
}

impl ClientInfo {
    fn echo_back(&self, command: Command) -> Message {
        Message {
            tags: None,
            prefix: Some(irc_proto::Prefix::Nickname(self.nick.clone(), self.user.clone(), self.host.clone())),
            command
        }
    }

    fn inbound_message(&self, from_username: String, target: Option<String>, txt: String) -> Message {
        Message {
            tags: None,
            prefix: Some(irc_proto::Prefix::Nickname(from_username.clone(), from_username.clone(), "irc.croquette".to_string())),
            command: Command::PRIVMSG(target.unwrap_or_else(|| self.nick.clone()), txt)
        }
    }

}

type IRCConn = Framed<TcpStream, IrcCodec>;

async fn respond(c: &mut IRCConn, code: irc_proto::Response, args: Vec<String>) -> Result<()> {
    use irc_proto::Prefix::ServerName;
    Ok(c.send(Message {
        tags: None,
        prefix: Some(ServerName("localhost".to_string())),
        command: Command::Response(code, args)
    }).await?)
}


async fn login(c: &mut IRCConn, from: String) -> Result<ClientInfo> {
    let (mut nick,mut user,mut pass) = (None, None, None);

    loop {
        let input = c.next()
            .await
            .ok_or(anyhow!("Connection closed"))??;

        match input.command {
            Command::NICK(n) => { nick = Some(n) },
            Command::PASS(p) => { pass = Some(p) },
            Command::USER(u, _mode, _realname) => { user = Some(u) },
            _ => { },
        }

        match(nick.is_some(), user.is_some(), pass.is_some()) {
            (true, true, true) => {
                respond(c, Response::RPL_WELCOME, vec!["Welcome to Rocket.chat via IRC proxy".into()]).await?;
                return Ok(ClientInfo { nick: nick.unwrap(), user: user.unwrap(), pass: pass.unwrap(), host: from });
            },

            (true, true, false) => {
                respond(c, Response::ERR_PASSWDMISMATCH, vec!["Please send your rocket authentication token".into()]).await?;
            }, 
            _ => {},
        }

    }

}

impl Proxy {

    pub fn new(bind: String, backend: String) -> Self {
        Self { bind, backend }
    }

    pub async fn run(&self) -> Result<()>  {
        let listener = TcpListener::bind(&self.bind).await?;
        eprintln!("Bound to {}", self.bind);

        loop {
            let (sock, peer) = listener.accept().await?;
            eprintln!("Accepted connection from {}", peer);

            let server = self.backend.clone();
            tokio::spawn(async move {
                match Self::handle_client(sock, peer, server).await {
                    Err(e) => eprint!("Connection terminated with error: {:?}", e),
                    _ => ()
                }
            });
        }

    }

    async fn handle_client(sock: TcpStream, peer: SocketAddr, server_addr: String) -> Result<()> {

        let mut client = irc_proto::IrcCodec::new("utf8")?
            .framed(sock);

        let clientinfo = login(&mut client, peer.to_string()).await?;
        eprintln!("Got clientinfo: {:?}", clientinfo);

        let mut back = rasta::Rasta::connect(&server_addr).await?;
        eprintln!("Backend connected");

        back.login(Credentials::Token(clientinfo.pass.clone())).await?;

        let mut session = Session::from(&mut back).await?;

        for room in &session.rooms {
            match room {
                Room::Chat { id, name, topic, ..} => {
                    let channel_name= format!("#{}", name);
                    client.send(clientinfo.echo_back(Command::JOIN(channel_name.clone(), None, None))).await?;
                    if let Some(topic) = topic {
                        respond(&mut client, Response::RPL_TOPIC, vec![channel_name, topic.clone()]).await?;
                    }
                },
                _ => {},
            }
        }

        let (mut client_up, client_down) = client.split();
        let mut client_down = client_down.fuse();
        let mut server_up = back.handle();

        let mut server_down = back.stream().fuse();

        loop {

            select! {

                msg = client_down.next() => {
                    let msg = msg.ok_or(anyhow!("Client closed connection"))??;
                    match msg {
                        Message { command: Command::PRIVMSG(target, payload),..} => {
                            if let Some(room) = target.strip_prefix('#')
                                .and_then(|chan| session.room_by_name(chan)) {
                                server_up.send_message(&room, payload).await?;
                            }
                        },
                        other => {
                            eprintln!("?? -> {:?}", other);
                        },
                    }                    
                },

                msg = server_down.next() => {
                    let msg = msg.ok_or(anyhow!("Server closed connection"))?;
                    match msg {
                        ServerMessage::Changed { fields: Some(mut obj), ..} => {
                            let args = obj.as_object_mut()
                                         .and_then(|m| m.remove("args"));
                            if let Some(Value::Array(args)) = args {
                                for obj_msg in args {
                                    if let Ok(msg) = serde_json::from_value::<rasta::schema::Message>(obj_msg) {
                                        if let Some(Room::Chat{name,..}) = session.room_by_id(&msg.rid) {
                                            let target = Some(format!("#{}", name));
                                            let out = clientinfo.inbound_message(msg.u.name, target, msg.msg);
                                            client_up.send(out).await?;
                                        }
                                    }
                                }
                            }
                        },
                        other => {
                            eprintln!("?? <- {:?}", other);
                        },
                    }
                },

            }

        }

    }

}

