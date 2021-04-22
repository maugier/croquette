use irc_proto::{Command, Message, Prefix};
use rasta::schema::{RoomEvent, RoomEventData, RoomExtraInfo};


pub enum ChatEvent {
    PublicMessage { from: String, room_name: String, msg: String },
    PrivateMessage { user: String, msg: String },
    TopicChange { user: String, room_name: String, topic: String },
}

impl ChatEvent {

    pub fn from_room_event(value: RoomEvent) -> Option<Self> {
        let evt = match value.args {
            ( RoomEventData {t: Some(t), msg, u, ..} 
            , RoomExtraInfo { room_name: Some(room_name) , room_type: 'c', ..}) 
                if &t == "room_changed_topic" => {
                    ChatEvent::TopicChange { user: u.username, room_name, topic: msg }
                },

            ( red, RoomExtraInfo { room_name: Some(room_name), room_type: 'c', ..}) => {
                    ChatEvent::PublicMessage { from: red.u.username, room_name, msg: red.msg } 
                },

            ( red, RoomExtraInfo { room_name: None, room_type: 'd',..}) => {
                    ChatEvent::PrivateMessage { user: red.u.username, msg: red.msg }
                }

            _ => return None,
        };
        Some(evt)
    }


    pub fn into_irc(self, local_nick: &str, remote_host: String) -> Message {
        match self {
            ChatEvent::TopicChange { user, room_name, topic } => {
                let chan = format!("#{}", room_name);
                Message { tags: None, prefix: Some(Prefix::Nickname(user.clone(), user, remote_host)) ,
                          command: Command::TOPIC(chan, Some(topic))}
            },
            ChatEvent::PublicMessage { from, room_name, msg, } => { 
                let chan = format!("#{}", room_name);
                Message { tags: None, prefix: Some(Prefix::Nickname(from.clone(), from, remote_host)),
                          command: Command::PRIVMSG(chan, msg)}
            },
            ChatEvent::PrivateMessage { user, msg } => {
                Message { tags: None, prefix: Some(Prefix::Nickname(user.clone(), user, remote_host)),
                          command: Command::PRIVMSG(local_nick.into(), msg)}
            },
        }
    }
}