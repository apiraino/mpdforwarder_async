#![deny(warnings)]

extern crate tokio;
extern crate tokio_io;

use std::sync::{Arc, Mutex};
use std::net::{Shutdown, SocketAddr};

use tokio::net::{TcpListener, TcpStream};
use tokio::prelude::*;
use tokio::io;
use tokio_io::codec::LinesCodec;

fn main() {
    let listen_addr = "127.0.0.1:6601".parse::<SocketAddr>().unwrap();
    let client = TcpListener::bind(&listen_addr).unwrap();
    println!("Listening on: {}", listen_addr);
    let done = client
        .incoming()
        .map_err(|e| println!("error accepting socket; error = {:?}", e))
        .for_each(move |client| {
            // check payload line by line
            let (client_writer, client_reader) = client.framed(LinesCodec::new()).split();
            let f = client_reader
                .into_future()
                .map_err(|_| println!("err"))
                .and_then(|(payload, client_reader)| {
                    let payload = payload.unwrap();
                    println!("payload={} client_reader={:?}", payload, client_reader);
                    let server_addr;
                    if payload == "quit" {
                        server_addr = "127.0.0.1:6602".parse::<SocketAddr>().unwrap();
                    } else {
                        server_addr = "127.0.0.1:6600".parse::<SocketAddr>().unwrap();
                    }
                    // Connect to the right server and forward the payload
                    let server_conn = TcpStream::connect(&server_addr);
                    let connected = server_conn.map_err(|e| eprintln!("Error: {}", e)).and_then(
                        |server_sock| {
                            println!("server connected");
                            // NOTE: without wrapping to MyTcpStream, we lose the "shutdown"
                            // when the stream back to the client ends, and the client hangs
                            let server_sock = MyTcpStream(Arc::new(Mutex::new(server_sock)));
                            let (server_writer, server_reader) =
                                server_sock.framed(LinesCodec::new()).split();
                            let forwarded_payload = client_reader
                                .forward(server_writer)
                                .map_err(|e| eprintln!("forward payload error={}", e))
                                .and_then(|res| {
                                    println!("forwarding payload to server: {:?}", res);
                                    let response_written = server_reader
                                        .forward(client_writer)
                                        .map_err(|err| eprintln!("[ERR] >>> {:?}", err))
                                        .then(|res| {
                                            println!("proxying response back to client: {:?}", res);
                                            println!("---");
                                            Ok(())
                                        });
                                    tokio::spawn(response_written)
                                });
                            tokio::spawn(forwarded_payload)
                        },
                    );
                    tokio::spawn(connected)
                });
            tokio::spawn(f)
        });
    tokio::run(done);
}

// This is a custom type used to have a custom implementation of the
// `AsyncWrite::shutdown` method which actually calls `TcpStream::shutdown` to
// notify the remote end that we're done writing.
#[derive(Clone, Debug)]
struct MyTcpStream(Arc<Mutex<TcpStream>>);

impl Read for MyTcpStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.lock().unwrap().read(buf)
    }
}

impl Write for MyTcpStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl AsyncRead for MyTcpStream {}

impl AsyncWrite for MyTcpStream {
    fn shutdown(&mut self) -> Poll<(), io::Error> {
        try!(self.0.lock().unwrap().shutdown(Shutdown::Write));
        Ok(().into())
    }
}
