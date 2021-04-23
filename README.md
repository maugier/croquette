# Croquette - Irc-Rocket bridge

## Features

 - [X] Public channels
 - [X] Direct messages
 - [X] Autojoin channels on connect
 - [ ] Userlist
 - [X] Joining channels
 - [ ] Leaving channels
 - [X] Changing channel topics
 - [X] Receive own messages from another connection
 - [ ] OTR bridging
 - [ ] Display GIFs names & urls in text
 - [ ] Render images as ascii art
 - [ ] SSL on IRC side

## Usage

Compile:

        cargo build --release

The binary will be in `./target/release/croquette`.

Run the proxy somewhere (ideally in your local machine),
giving it a local listen address and a target server:
        ./croquette 127.0.0.1:6667 rocket.example.com


Acquire an authentication token from Rocket, by going into 
the "My Account" page, "Personal Access Tokens", and issuing
a new token. Configure your IRC client to use this token as a
connection password.


Multiple people can use the bridge at the same time, with different credentials.
This is why the token needs to be provided by the client on each connection.
