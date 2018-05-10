// #![deny(warnings)]

extern crate encoding;
extern crate tokio;
extern crate tokio_io;

use std::env;
use std::sync::{Arc, Mutex};
use std::net::{Shutdown, SocketAddr};

use encoding::{EncoderTrap, Encoding};
use encoding::all::ASCII;

use tokio::net::{TcpListener, TcpStream};
use tokio::prelude::*;
use tokio::io;
use tokio_io::codec::LinesCodec;

fn main() {
    match env::args().nth(1) {
        Some(arg) => process_arguments(&arg),
        None => println!("Please choose proxy [1,2,3,...]"),
    }
}

fn process_arguments(arg: &str) {
    let listen_addr = "127.0.0.1:6601".parse::<SocketAddr>().unwrap();
    let client = TcpListener::bind(&listen_addr).unwrap();
    println!("Listening on: {}", listen_addr);
    println!("Running proxy {}", arg);
    match arg {
        "1" => {
            proxy_1(client);
        }
        "2" => {
            proxy_2(client);
        }
        "3" => {
            simple_client(client);
        }
        _ => println!("Please choose a proxy"),
    }
}

// A simple client that connects to a server and reads its answer
fn simple_client(client: TcpListener) {
    let server_addr = "127.0.0.1:6600".parse::<SocketAddr>().unwrap();
    let server_conn = TcpStream::connect(&server_addr);
    let task = server_conn
        .map_err(|err| eprintln!("{}", err))
        .and_then(|socket| {
            println!("conn successful");
            let write_all = tokio_io::io::write_all(socket, "status\r\n".as_bytes())
                .map_err(|err| eprintln!("{}", err))
                .and_then(|(stream, payload)| {
                    // socket is never closed from MPD
                    // therefore the connection hangs until timeout
                    // let read_resp = tokio_io::io::read_to_end(stream, Vec::new())
                    //     .map_err(|_| {})
                    //     .and_then(|(_stream, response)| {
                    //         println!("response={}", String::from_utf8(response).unwrap());
                    //         Ok(())
                    //     });

                    // wrap the stream read into a Bufreader
                    // then read line by line
                    // until the EOF marker ("OK") is received
                    let reader = std::io::BufReader::new(stream);
                    let read_resp = tokio_io::io::lines(reader)
                        .take_while(|line| future::ok(line != "OK"))
                        .map_err(|err| eprintln!("{}", err))
                        .for_each(|line| {
                            println!("got line = {}", line);
                            Ok(())
                        });
                    tokio::spawn(read_resp)
                });
            tokio::spawn(write_all)
        });
    tokio::run(task);
}

// Proxy inspects command received, sends command to server, read response, sends back a string to client
fn proxy_1(client: TcpListener) {
    let done = client
        .incoming()
        .map_err(|_| println!("err"))
        .for_each(move |client| {
            let (client_sink, client_stream) = client.framed(LinesCodec::new()).split();
            let f = client_stream
                .into_future()
                .map_err(|_| {})
                .map(|(payload, client_stream)| {
                    let payload = payload.unwrap().clone();
                    println!("got payload={:?}", payload);

                    // choose destination server
                    let server_addr;
                    if payload == "quit" {
                        server_addr = "127.0.0.1:6602".parse::<SocketAddr>().unwrap();
                    } else {
                        server_addr = "127.0.0.1:6600".parse::<SocketAddr>().unwrap();
                    }

                    // connect to server
                    // let server_addr = "127.0.0.1:6600".parse::<SocketAddr>().unwrap();
                    let server_conn = TcpStream::connect(&server_addr);

                    let connected = server_conn.map_err(|_| {}).and_then(move |server_sock| {
                        println!("connected: {:?}", server_sock);
                        let mut command_bytes =
                            ASCII.encode(&payload, EncoderTrap::Strict).unwrap();
                        command_bytes.push('\r' as u8);
                        command_bytes.push('\n' as u8);

                        let write_all = tokio_io::io::write_all(server_sock, command_bytes)
                            .map_err(|err| eprintln!("{}", err))
                            .and_then(|(stream, _payload)| {
                                let reader = std::io::BufReader::new(stream);
                                let read_resp = tokio_io::io::lines(reader)
                                    .take_while(|line| future::ok(line != "OK"))
                                    .map_err(|err| eprintln!("{}", err))
                                    .for_each(|line| {
                                        println!("got line = {}", line);
                                        Ok(())
                                    })
                                    .then(|res| {
                                        println!("res={:?}", res);
                                        // proxy the line back to client
                                        let client_reunited = client_sink
                                            .reunite(client_stream)
                                            .unwrap()
                                            .into_inner();
                                        let pong =
                                            tokio_io::io::write_all(client_reunited, b"finished\n")
                                                .map_err(|err| eprintln!("{}", err))
                                                .then(|_| Ok(()));
                                        tokio::spawn(pong)
                                    });
                                tokio::spawn(read_resp)
                            });
                        tokio::spawn(write_all)
                    });
                    tokio::run(connected)
                });
            tokio::spawn(f)
        });

    tokio::run(done);
}

// Proxy sends command to server, forwards response without inspection
fn proxy_2(client: TcpListener) {
    let done = client
        .incoming()
        .map_err(|e| println!("error accepting socket; error = {:?}", e))
        .for_each(move |client| {
            let server_addr = "127.0.0.1:6600".parse::<SocketAddr>().unwrap();
            let server_conn = TcpStream::connect(&server_addr);
            let connected = server_conn.map_err(|e| eprintln!("Error: {}", e)).and_then(
                move |server_sock| {
                    // NOTE: without wrapping to MyTcpStream, we lose the "shutdown"
                    // when the stream back to the client ends, and the client hangs
                    let server_sock = MyTcpStream(Arc::new(Mutex::new(server_sock)));
                    let (client_writer, client_reader) = client.framed(LinesCodec::new()).split();
                    let (server_writer, server_reader) =
                        server_sock.framed(LinesCodec::new()).split();
                    let forwarded_payload = client_reader
                        .forward(server_writer)
                        .map_err(|e| eprintln!("forward payload error={}", e))
                        .and_then(|_| {
                            let response_written = server_reader
                                .forward(client_writer)
                                .map_err(|_| {})
                                .and_then(|_| Ok(()));
                            tokio::spawn(response_written)
                        });
                    tokio::spawn(forwarded_payload)
                },
            );
            tokio::spawn(connected)
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
