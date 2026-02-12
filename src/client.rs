use chat::chat_service_client::ChatServiceClient;
use chat::{ConnectionRequest, Message};
use clap::Parser;
use std::error::Error;
use tokio::io::{AsyncBufReadExt, BufReader, stdin};
use tonic::Streaming;

pub mod chat {
    tonic::include_proto!("chat");
}

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(index = 1)]
    username: String,

    #[arg(index = 2)]
    server_url: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();

    // Create a gRPC channel, open HTTP/2 connection
    let mut client = ChatServiceClient::connect(match args.server_url.starts_with("https://") {
        true => args.server_url,
        false => format!("https://{}", args.server_url),
    }).await?;

    let request = tonic::Request::new(ConnectionRequest {
        username: args.username.clone(),
    });

    // HTTP/2 frame receive, decode with prost, convert to Message - <username> connected to the server
    let mut stream: Streaming<Message> = client.connect_to_server(request).await?.into_inner();

    println!("Connected to server successfully!");

    {
        // create async buffered reader, return a stream of lines
        let mut reader_lines = BufReader::new(stdin()).lines();

        // create a new async task (green thread)
        tokio::spawn(async move {
            loop {
                // wait for multiple async operations at the same time
                tokio::select! {
                    Ok(Some(line)) = reader_lines.next_line() => {
                        let request = tonic::Request::new(Message {
                            username: args.username.clone(),
                            message: line,
                        });
                        if let Err(_) =  client.send_message(request).await {
                            eprintln!("Failed to send message to server.");
                            return;
                        }
                    }
                    Ok(Some(line)) = stream.message() => {
                        println!("[{}]: {}", line.username, line.message);
                    }
                }
            }
        }).await?;
    }

    Ok(())
}