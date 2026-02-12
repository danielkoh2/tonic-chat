use chat::chat_service_server::{ChatService, ChatServiceServer};
use chat::{ConnectionRequest, Empty, Message};
use std::collections::HashMap;
use std::error::Error;
use std::net::SocketAddr;
use std::pin::Pin;
use std::str::FromStr;
use std::sync::Arc;
use clap::Parser;
use tokio::sync::{RwLock, mpsc};
use tokio_stream::Stream;
use tonic::{Request, Response, Status};
use tonic::transport::Server;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(index = 1)]
    port: String,
}

pub mod chat {
    tonic::include_proto!("chat");
}

#[derive(Debug)]
struct ChatState {
    peers: HashMap<String, mpsc::Sender<Message>>,
}

impl ChatState {
    pub fn new() -> Self {
        Self {
            peers: HashMap::new(),
        }
    }

    pub async fn broadcast(&self, message: &Message) {
        for (name, tx) in &self.peers {
            match tx.send(message.clone()).await {
                Ok(_) => {}
                Err(_) => {
                    println!("Failed to send message to {}.", name);
                }
            }
        }
    }
}

#[derive(Debug)]
struct ChatServer {
    state: Arc<RwLock<ChatState>>,
}

impl ChatServer {
    pub fn new(state: Arc<RwLock<ChatState>>) -> Self {
        Self { state }
    }
}

#[tonic::async_trait]
impl ChatService for ChatServer {
    type ConnectToServerStream = Pin<Box<dyn Stream<Item = Result<Message, Status>> + Send + Sync + 'static>>;

    async fn connect_to_server(
        &self,
        request: Request<ConnectionRequest>
    ) -> Result<Response<Self::ConnectToServerStream>, Status> {
        let username = request.into_inner().username;

        let (stream_tx, stream_rx) = mpsc::channel(1);
        let (tx, mut rx) = mpsc::channel(1);

        {
            self.state.write().await.peers.insert(username.clone(), tx);
        }

        println!("{} connected to the server.", username);

        {
            let shared_clone = self.state.clone();
            tokio::spawn(async move {
                while let Some(message) = rx.recv().await {
                    match stream_tx.send(Ok(message)).await {
                        Ok(_) => {}
                        Err(_) => {
                            println!("Failed to send message to {}.", &username);
                            shared_clone.write().await.peers.remove(&username);
                        }
                    }
                }
            });
        }

        Ok(Response::new(Box::pin(
            tokio_stream::wrappers::ReceiverStream::new(stream_rx),
        )))
    }

    async fn send_message(&self, request: Request<Message>) -> Result<Response<Empty>, Status> {
        let request_data = request.into_inner();
        let username = request_data.username;
        let message = request_data.message;
        let message = Message { username, message };
        self.state.read().await.broadcast(&message).await;

        Ok(Response::new(Empty {}))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();

    let address: SocketAddr = SocketAddr::from_str(&format!("127.0.0.1:{}",&args.port))?;

    let state = Arc::new(RwLock::new(ChatState::new()));
    let chat_server = ChatServer::new(state.clone());

    Server::builder()
        .add_service(ChatServiceServer::new(chat_server))
        .serve(address)
        .await?;

    Ok(())
}