use std::io;
use std::fs::File;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use log::*;
use simplelog::*;

use rand::prelude::*;

use tokio::prelude::*;
use tokio::net::TcpListener;
use tokio::codec::Decoder;

use futures::stream;
use futures::sync::mpsc;
use futures::sync::mpsc::UnboundedSender;

use core::{AddrResponse, ServerToClientCodec};

fn gen_sock_addr() -> SocketAddr {
    let ip = IpAddr::V4(Ipv4Addr::new(
        rand::random::<u8>(),
        rand::random::<u8>(),
        rand::random::<u8>(),
        rand::random::<u8>(),
    ));
    let port = rand::random::<u16>();
    SocketAddr::new(ip, port)
}

fn main() {
    CombinedLogger::init(
        vec![
            TermLogger::new(LevelFilter::Info, Config::default()).unwrap(),
            WriteLogger::new(
                LevelFilter::Info,
                Config::default(),
                File::create("/tmp/maidsafe-test-server.log").unwrap()),
        ]
    ).unwrap();

    let mut args = std::env::args();
    let program = args.next().unwrap();
    let (host, port) = match (args.next(), args.next()) {
        (Some(host), Some(port)) => (host, port),
        _ => return println!("Usage: {} <host> <port>", program),
    };

    let addr = format!("{}:{}", host, port).parse().unwrap();
    let listener = TcpListener::bind(&addr)
        .expect(&format!("Could not bind to {}", addr));

    let server = listener
        .incoming()
        .map_err(|e| error!("Server error: {}", e))
        .for_each(move |stream| {
            info!("Connected to {:?}", stream);

            let addr = stream.peer_addr().unwrap();
            let (writer, reader) = ServerToClientCodec.framed(stream).split();
            let client = reader
                .map(move |req| {
                    info!("Received request {:?} from {}", req, addr);
                    let mut addrs = Vec::with_capacity(req.num_addrs as usize);
                    for _ in 0..req.num_addrs {
                        addrs.push(gen_sock_addr());
                    }
                    info!("Generated addrs: {:?}", addrs);
                    AddrResponse { addrs }
                })
                .forward(writer)
                .map_err(|e| error!("Client error: {}", e))
                .and_then(|(_reader, _writer)| Ok(()));

            tokio::spawn(client)
        });

    tokio::run(server);
}

