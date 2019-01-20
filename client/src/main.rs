use std::io;
use std::thread;
use std::fs::File;

use log::*;
use simplelog::*;

use tokio::prelude::*;
use tokio::net::TcpStream;
use tokio::codec::Decoder;

use futures::sync::mpsc;

use core::{Request, Response, ClientToServerCodec};

fn main() {
    let mut args = std::env::args();
    let program = args.next().unwrap();
    let (host, port) = match (args.next(), args.next()) {
        (Some(host), Some(port)) => (host, port),
        _ => return println!("Usage: {} <host> <port>", program),
    };

    WriteLogger::new(
        LevelFilter::Info,
        Config::default(),
        File::create(format!("/tmp/maidsafe-test-client.log")).unwrap(),
    );

    let (stdin_chan, stdin_port) = mpsc::unbounded();
    let (stdout_chan, stdout_port) = std::sync::mpsc::channel();

    thread::spawn(move || ui_thread(stdin_chan, stdout_port));

    let addr = format!("{}:{}", host, port).parse().unwrap();
    let connect = TcpStream::connect(&addr);

    let session = connect.and_then(move |stream| {
        info!("Starting session");
        let (writer, reader) = ClientToServerCodec.framed(stream).split();

        let write = stdin_port
            .map_err(|()| unreachable!("stdin_port can't fail"))
            .fold(writer, |writer, req| {
                info!("Sending request: {:?}", req);
                if req.num_addrs == 0 {
                    // TODO: gracefully shutdown Tokio runtime.
                    std::process::exit(0);
                } else {
                    writer.send(req)
                }
            })
            .map(|_| ());

        let read = reader.for_each(move |resp| {
            info!("Got response: {:?}", resp);
            stdout_chan.send(resp).unwrap();
            Ok(())
        });

        read.select(write).map(|_| ()).map_err(|(err, _)| err)
    });

    tokio::run(session.map_err(|_e| ()));
}

fn ui_thread(
    mut stdin_chan: mpsc::UnboundedSender<Request>,
    stdout_port: std::sync::mpsc::Receiver<Response>,
) {
    info!("Starting stdio thread");
    loop {
        let mut buf = String::new();
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
        let req = Request { num_addrs };
        stdin_chan = match stdin_chan.send(req).wait() {
            Ok(tx) => tx,
            Err(e) => {
                error!("Stdin error: {}", e);
                break;
            }
        };
        match stdout_port.recv() {
            Ok(resp) => {
                for addr in resp.addrs {
                    println!("{}", addr);
                }
            },
            Err(_) => (), // TODO
        }
        if num_addrs == 0 {
            info!("Exiting program");
            break;
        }
    }
}

