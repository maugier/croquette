use std::net::SocketAddr;
use anyhow::{Result, anyhow};
use tokio::net::{
    TcpListener,TcpStream,
};
use tokio_util::codec::{Decoder, Framed};
use irc_proto::{Command, IrcCodec, Message, Prefix, Response};
use futures::{SinkExt, StreamExt, select, stream::SplitSink};
use rasta::{Credentials, Handle, ServerMessage, schema::{Room, ShortUser, UserID}, session::Session};
use crate::{event::ChatEvent, util::lazy_zip};
use log::{trace,debug,info,warn,error};


#[derive(Debug)]
pub struct ProxyListener {
    bind: String,
    backend: String,
}

#[derive(Debug)]
pub struct ClientInfo {
    nick: String,
    user: String,
    pass: String,
    host: String,
}

impl Into<Prefix> for &ClientInfo {
    fn into(self) -> Prefix {
        Prefix::Nickname(self.nick.clone(), self.user.clone(), self.host.clone())
    }
}

impl std::fmt::Display for ClientInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}!{}@{}", self.nick, self.user, self.host)
    }
}

impl ClientInfo {
    fn echo_back(&self, command: Command) -> Message {
        Message {
            tags: None,
            prefix: Some(self.into()),
            command
        }
    }

    /*
    fn inbound_message(&self, from_username: String, target: Option<String>, txt: String) -> Message {
        Message {
            tags: None,
            prefix: Some(irc_proto::Prefix::Nickname(from_username.clone(), from_username.clone(), "irc.croquette".to_string())),
            command: Command::PRIVMSG(target.unwrap_or_else(|| self.nick.clone()), txt)
        }
    }
    */

}

fn server_response(server_name: &str, user: String, code: Response, mut args: Vec<String>) -> Message {
    args.insert(0, user);
    Message {
        tags: None,
        prefix: Some(Prefix::ServerName(server_name.to_string())),
        command: Command::Response(code, args)
    }
}

fn build_userlist(user: &str, server: &str, room: &Room, users: &[ShortUser]) -> Vec<Message> {

    let (room_name, modechar) = match room {
        Room::Chat { name, .. } => (name, '='),
        Room::Private { name, ..} => (name, '*'),
        _ => return vec![],
    };

    let channel = format!("#{}", room_name);
    let mut output = Vec::new();

    let mut userlist = String::new();
    for person in users {
        if userlist.len() > 512 {
            output.push(server_response(server, user.into(),
            Response::RPL_NAMREPLY, vec![modechar.into(), channel.clone(), userlist]));
            userlist = String::new();
        }

        if userlist.len() > 0 { userlist += " "; }
        userlist += &person.username;

    }

    if userlist.len() > 0 {
        output.push(server_response(server, user.into(),
            Response::RPL_NAMREPLY, vec![modechar.into(), channel.clone(), userlist]));
    }

    output.push(server_response(server, user.into(),
     Response::RPL_ENDOFNAMES, vec![channel, "End of /NAMES list.".into()]));

    output

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


async fn login(c: &mut IRCConn, host: String) -> Result<ClientInfo> {
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

        match (&nick, &user, &pass) {
            (Some(nick), Some(user), Some(pass)) => {
                respond(c, Response::RPL_WELCOME, vec![nick.clone(), "Welcome to Rocket.chat via IRC proxy".into()]).await?;
                return Ok(ClientInfo { nick: nick.clone(), user: user.clone(), pass: pass.clone(), host });
            },

            (Some(nick), Some(_), None) => {
                respond(c, Response::ERR_PASSWDMISMATCH, vec![nick.clone(), "Please send your rocket authentication token".into()]).await?;
            }, 
            _ => {},
        }

    }

}

pub struct Proxy {
    clientinfo: ClientInfo,
    userid: UserID,
    session: Session,
    server_up: Handle,
    client_up: SplitSink<IRCConn, Message>,
    server_addr: String,
}

impl Proxy {

    async fn run(sock: TcpStream, peer: SocketAddr, server_addr: String) -> Result<()> {

        let mut client = irc_proto::IrcCodec::new("utf8")?
            .framed(sock);

        let clientinfo = login(&mut client, peer.ip().to_string()).await?;
        debug!("Client logged in as {}", clientinfo);

        let mut back = rasta::Rasta::connect(&server_addr).await?;
        info!("Backend connected");

        let userid: UserID = match back.login(Credentials::Token(clientinfo.pass.clone())).await? {
            None => {
                respond(&mut client, Response::ERR_PASSWDMISMATCH,
                vec![
                    clientinfo.nick.clone(),
                    "Backend server rejected token".to_string(),
                    ]).await?;
                return Err(anyhow!("Login failed."))
            },
            Some(login) => {
                login.id
            }
        };
        
        let mut server_up = back.handle();
        let session = Session::from(&mut back).await?;

        for room in &session.rooms {
            match room {
                Room::Chat { name, topic, ..} => {
                    let channel_name= format!("#{}", name);
                    client.send(clientinfo.echo_back(Command::JOIN(channel_name.clone(), None, None))).await?;
                    if let Some(topic) = topic {
                        respond(&mut client, Response::RPL_TOPIC, vec![clientinfo.nick.to_string(), channel_name, topic.clone()]).await?;
                    }

                    if let Ok(users) = server_up.get_room_users(room).await {

                        trace!("Got userlist: {:?}", users);

                        for msg in build_userlist(&clientinfo.nick, &server_addr, &room, &users) {
                            client.feed(msg).await?;
                        }
                        client.flush().await?;
                    }

                },
                _ => {},
            }
        }

        back.subscribe_my_messages().await?;

        let (client_up, client_down) = client.split();
        let mut client_down = client_down.fuse();


        let mut server_down = back.stream().fuse();

        let mut proxy = Proxy { clientinfo, userid, session,
            server_up, client_up, server_addr };

        loop {

            select! {

                msg = client_down.next() => {
                    let msg = msg.ok_or(anyhow!("Client closed connection"))??;
                    proxy.client_message(msg).await?;
                },

                msg = server_down.next() => {
                    let msg = msg.ok_or(anyhow!("Server closed connection"))?;
                    proxy.server_message(msg).await?;
                },

            }

        }

    }


    async fn client_message(&mut self, msg: Message) -> Result<()> {
        match msg {
            Message { command: Command::JOIN(chanlist, keys, _), ..} => {
                let chanlist = chanlist.split(",");
                let keys = keys.as_ref().map(|k| k.split(",").map(str::to_owned));
                for (chan, key) in lazy_zip(chanlist, keys) {
                    if let Some(name) = chan.strip_prefix('#') {
                        debug!("Joining {} with key {:?}", chan, key);
                        if let Some(rid) = self.server_up.lookup_room_id(name.into()).await? {
                            if self.server_up.join_room(rid, key).await? {
                                self.client_up.send(self.clientinfo.echo_back(Command::JOIN(chan.into(), None, None))).await?
                         }
                        } else {
                            warn!("Room not found: #{}", chan);
                        }
                    } else {
                        warn!("Other room types not supported");
                    }
                }
            }
            Message { command: Command::PING(a,b), ..} => {
                self.client_up.send(Message { tags: None,
                    prefix: Some(Prefix::ServerName(self.server_addr.clone())),
                    command: Command::PONG(a,b)
                }).await?
            },

            Message { command: Command::PRIVMSG(target, payload),..} => {
                let session = &self.session;
                if let Some(room) = target.strip_prefix('#')
                    .and_then(|chan| session.room_by_name(chan)) {
                    self.server_up.send_message(&room, payload).await?;
                } else {
                    let direct = self.session.direct_room(&mut self.server_up, &target).await?;
                    self.server_up.send_message(&direct, payload).await?;
                }
            },
            Message { command: Command::TOPIC(target, topic),..} => {
                let session = &self.session;
                if let Some(room) = target.strip_prefix('#')
                    .and_then(|chan| session.room_by_name(chan)) {
                        if self.server_up.set_topic(room, topic.clone()).await? {
                            self.client_up.send(self.clientinfo.echo_back(Command::TOPIC(target, topic))).await?
                        }
                    }
            },
            Message { command: Command::AWAY(reason),..} => {
                self.server_up.set_away(reason.is_some()).await?;    
            },
            other => {
                warn!("Unsupported IRC command: {:?}", other);
            },
        };
        Ok(())
    }
    
    async fn server_message(&mut self, msg: ServerMessage) -> Result<()> {
        match msg {
            ServerMessage::Changed { fields: Some(obj), ..} => {
                if let Some(evt) = ChatEvent::from_room_event(serde_json::from_value(obj.clone())?) {
                    self.client_up.send(evt.into_irc(&self.clientinfo.nick, self.server_addr.clone())).await?
                } else {
                    warn!("Unsupported Rocket change: {}", obj);
                }                      
            },
            ServerMessage::Updated {..} => {},
            other => {
                warn!("Unsupported Rocket event: {}", other.pretty());
            },
        };
        Ok(())
    }


}

impl ProxyListener {

    pub fn new(bind: String, backend: String) -> Self {
        Self { bind, backend }
    }

    pub async fn run(&self) -> Result<()>  {
        let listener = TcpListener::bind(&self.bind).await?;
        info!("Bound to {}", self.bind);

        loop {
            let (sock, peer) = listener.accept().await?;
            info!("Accepted connection from {}", peer);

            let server = self.backend.clone();
            tokio::spawn(async move {
                match Proxy::run(sock, peer, server).await {
                    Err(e) => error!("Connection terminated with error: {:?}", e),
                    _ => ()
                }
            });
        }

    }

}

