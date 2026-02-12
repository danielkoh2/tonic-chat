# Tonic-Chat
Rust Chat using tonic gRPC

## Install
>rustup update

>git clone https://github.com/danielkoh2/tonic-chat.git

>cargo build

## Usage

**Server:**
>cargo run --bin server 3000

**Client1:**
>cargo run --bin client Alice 127.0.0.1:3000

**Client2:**
>cargo run --bin client Bob 127.0.0.1:3000