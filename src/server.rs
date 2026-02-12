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
// global shared memory, map of username -> mpsc, (mpsc: multi-provider, single consumer channel)
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
            // must use message.clone because multiple receivers exist
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
    //gRPC server runs multiple concurrent requests
    //each RPC call may run on different async tasks, so need share state across tasks.
    state: Arc<RwLock<ChatState>>,
}

impl ChatServer {
    pub fn new(state: Arc<RwLock<ChatState>>) -> Self {
        Self { state }
    }
}

#[tonic::async_trait]
impl ChatService for ChatServer {
    //tonic requirement, Pin<Box<dyn Stream<Item = Result<Message, Status>>>>
    type ConnectToServerStream = Pin<Box<dyn Stream<Item = Result<Message, Status>> + Send + Sync + 'static>>;

    // client sends username, server returns a streaming channel due to proto
    async fn connect_to_server(
        &self,
        request: Request<ConnectionRequest>
    ) -> Result<Response<Self::ConnectToServerStream>, Status> {
        let username = request.into_inner().username;

        //Channel A, channel to receive internal messages.
        let (tx, mut rx) = mpsc::channel(1);
        //Channel B, channel to send messages into gRPC stream
        let (stream_tx, stream_rx) = mpsc::channel(1);

        {
            //Modify HashMap
            self.state.write().await.peers.insert(username.clone(), tx);
        }

        println!("{} connected to the server.", username);
        let join_message = Message {
            username: "System".to_string(),
            message: format!("{} joined this chat", username),
        };
        self.state.read().await.broadcast(&join_message).await;

        {
            let shared_clone = self.state.clone();
            tokio::spawn(async move {
                // wait for messages from broadcast
                while let Some(message) = rx.recv().await {
                    // forward them into gRPC stream
                    match stream_tx.send(Ok(message)).await {
                        Ok(_) => {}
                        Err(_) => {
                            println!("Failed to send message to {}.", &username);
                            {
                                shared_clone.write().await.peers.remove(&username);
                            }
                            let leave_message = Message {
                                username: "System".to_string(),
                                message: format!("{} left the chat", username),
                            };
                            shared_clone.read().await.broadcast(&leave_message).await;
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