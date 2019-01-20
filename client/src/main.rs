use std::io;
use std::thread;
use std::fs::File;

use log::*;
use simplelog::*;

use tokio::io::shutdown;
use tokio::prelude::*;
use tokio::net::TcpStream;
use tokio::codec::Decoder;

use futures::sync::mpsc;

use core::{Request, ClientToServerCodec};

fn main() {
    let mut args = std::env::args();
    let program = args.next().unwrap();
    let (host, port) = match (args.next(), args.next()) {
        (Some(host), Some(port)) => (host, port),
        _ => return println!("Usage: {} <host> <port>", program),
    };

    CombinedLogger::init(
        vec![
            WriteLogger::new(
                LevelFilter::Info,
                Config::default(),
                File::create(format!("/tmp/maidsafe-test-client.log")).unwrap(),
            ),
            //TermLogger::new(
                //LevelFilter::Info,
                //Config::default(),
            //).unwrap(),
        ]
    ).unwrap();

    let (stdin_chan, stdin_port) = mpsc::unbounded();

    thread::spawn(move || {
        let mut stdin_chan = stdin_chan;
        info!("Starting stdio thread");
        loop {
            // TODO: don't reallocate string every time.
            let mut buf = String::new();
            // TODO: stdin error handling?
            print!("> ");
            io::stdout().flush().unwrap();
            io::stdin().read_line(&mut buf).unwrap();
            let num_addrs = match buf.trim().parse() {
                Ok(n) => n,
                Err(_) => {
                    println!("Input must be an integer");
                    continue;
                },
            };
            let msg = Request { num_addrs };
            stdin_chan = match stdin_chan.send(msg).wait() {
                Ok(tx) => tx,
                Err(e) => {
                    error!("Stdin error: {}", e);
                    break;
                }
            };
            if num_addrs == 0 {
                info!("Exiting program");
                break;
            }
        }
    });

    let addr = format!("{}:{}", host, port).parse().unwrap();
    let connect = TcpStream::connect(&addr);

    let session = connect.and_then(move |stream| {
        info!("Starting session");
        let (writer, reader) = ClientToServerCodec.framed(stream).split();

        let write = stdin_port
            .map_err(|()| unreachable!("stdin_port can't fail"))
            .fold(writer, |writer, msg| {
                info!("Sending msg: {:?}", msg);
                if msg.num_addrs == 0 {
                    // TODO: gracefully shutdown Tokio runtime.
                    std::process::exit(0);
                } else {
                    writer.send(msg)
                }
            })
            .map(|_| ());

        let read = reader.for_each(move |msg| {
            info!("Got msg: {:?}", msg);
            println!("Addresses: {:?}", msg.addrs);
            Ok(())
        });

        read.select(write).map(|_| ()).map_err(|(err, _)| err)
    });

    tokio::run(session.map_err(|_e| ()));
}

