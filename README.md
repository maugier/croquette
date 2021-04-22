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

## Usage

Compile:

        cargo build --release

The binary will be in `./target/release/croquette`.

Acquire an authentication token from Rocket, by going into your user
profile page in the web interface.

Run the proxy, giving it a local listen address and a target server:

        ./croquette 127.0.0.1:6667 rocket.example.com

Configure your IRC client to use the rocket token as a password.
