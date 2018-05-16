// #![deny(warnings)]

extern crate encoding;
extern crate tokio;
extern crate tokio_io;

use std::env;
use std::net::{Shutdown, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use encoding::all::ASCII;
use encoding::{EncoderTrap, Encoding};

use tokio::io;
use tokio::net::{TcpListener, TcpStream};
use tokio::prelude::*;
use tokio_io::codec::LinesCodec;

// A custom type used to have a custom implementation of the
// `AsyncWrite::shutdown` method which actually calls `TcpStream::shutdown` to
// notify the remote end that we're done writing.
#[derive(Clone)]
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

macro_rules! println_marked {
    ($param:expr) => {
        let ts = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
        let in_ms = ts.as_secs() * 1000 + ts.subsec_nanos() as u64 / 1_000_000;
        println!("[{}] {}", in_ms, $param);
    };
}

fn main() {
    let listen_addr = "127.0.0.1:6601".parse::<SocketAddr>().unwrap();
    let client = TcpListener::bind(&listen_addr).unwrap();
    println!("Listening on: {}", listen_addr);

    let arg = env::args().nth(1).unwrap();
    match &*arg {
        "simple_client" => simple_client(),
        "proxy_1" => proxy_1(client),
        "proxy_2" => proxy_2(client),
        // "fix_time_wait" => fix_time_wait(client),
        _ => {
            eprintln!("Function {} not defined", arg);
        }
    }
}

// A simple client that connects to a server and reads its answer
fn simple_client() {
    let server_addr = "127.0.0.1:6600".parse::<SocketAddr>().unwrap();
    let server_conn = TcpStream::connect(&server_addr);
    let task = server_conn
        .map_err(|err| eprintln!("{}", err))
        .and_then(|socket| {
            println!("conn successful");
            let write_all = tokio_io::io::write_all(socket, "status\r\n".as_bytes())
                .map_err(|err| eprintln!("{}", err))
                .and_then(|(stream, _payload)| {
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

// Proxy sends command to server, forwards response without inspection
fn proxy_1(client: TcpListener) {
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

// when reading from a stream, the client connection hangs in TIME_WAIT
// if we don't do anything after reading
fn _fix_time_wait(client: TcpListener) {
    let client_conns = client.incoming().map_err(|_| println!("err"));
    let process_reqs = client_conns.for_each(move |client| {
        println!("Request from {:?}", client);

        // split client streams
        let (client_sink, client_stream) = client.framed(LinesCodec::new()).split();
        let client_stream_fut = client_stream.into_future();
        let read_payload = client_stream_fut.then(|result| {
            println!("we're done here, closing socket.");
            let (_payload, stream) = result.unwrap();
            let client_reunited = client_sink.reunite(stream).unwrap().into_inner();

            // send something back to the client
            // this will close the client stream
            let pong = tokio_io::io::write_all(client_reunited, b"bye\n");
            tokio::spawn(pong.map_err(|err| eprintln!("{}", err)).then(|_| Ok(())))

            // this will leave the client stream in TIME_WAIT
            // client_reunited.shutdown(Shutdown::Both).map_err(|_| {})
        });
        tokio::spawn(read_payload)
    });
    tokio::run(process_reqs.map_err(|_| {}));
}

/*
- read client payload
- connect to server
- send msg to server
- read server response (line by line)
- send a msg back to client
*/
fn proxy_2(client: TcpListener) {
    let mut counter: i32 = 0;
    let get_counter = |num| num + 1;
    let client_conns = client.incoming().map_err(|err| eprintln!("{}", err));
    let process_reqs = client_conns.for_each(move |client| {
        let conn_counter = counter;
        counter = get_counter(counter);
        println!("[CONN{}] [0] Request from {:?}", conn_counter, client);

        // split client streams
        let (client_sink, client_stream) = client.framed(LinesCodec::new()).split();
        let client_stream_fut = client_stream
            .into_future()
            .map_err(|err| eprintln!("{:?}", err));
        let read_payload = client_stream_fut
            .map(move |(payload, stream)| {
                let payload = payload.unwrap();
                println!(
                    "[CONN{}] [1] payload rcvd '{}', calculating server port",
                    conn_counter, payload
                );

                // choose destination server
                let server_addr;
                if payload == "quit" {
                    server_addr = "127.0.0.1:6602".parse::<SocketAddr>().unwrap();
                } else {
                    server_addr = "127.0.0.1:6600".parse::<SocketAddr>().unwrap();
                }
                (payload, server_addr, stream)
            })
            .and_then(move |(payload, server_addr, client_stream_inner)| {
                println!(
                    "[CONN{}] [2] connecting to server {}",
                    conn_counter, server_addr
                );

                let server_conn = TcpStream::connect(&server_addr)
                    .map(|stream| stream)
                    .map_err(|err| eprintln!("{}", err));
                let connected = server_conn.and_then(move |server_sock| {
                    println!(
                        "[CONN{}] [3] connected to server {:?}",
                        conn_counter, server_sock
                    );
                    // format new payload
                    let mut command_bytes = ASCII.encode(&payload, EncoderTrap::Strict).unwrap();
                    command_bytes.push('\r' as u8);
                    command_bytes.push('\n' as u8);

                    // send msg to server *and* read response
                    let write_payload = tokio_io::io::write_all(server_sock, command_bytes)
                        .map_err(|err| eprintln!("{}", err));
                    let response_reader = write_payload.and_then(move |(stream, msg)| {
                        println!(
                            "[CONN{}] [4] msg '{}' sent to server",
                            conn_counter,
                            String::from_utf8(msg).unwrap().trim()
                        );
                        let reader = std::io::BufReader::new(stream);
                        let resp_lines = tokio_io::io::lines(reader);
                        let line_iter = resp_lines
                            .take_while(|line| future::ok(line != "OK"))
                            .map_err(|err| eprintln!("{}", err));
                        let line_iter = line_iter
                            .for_each(|line| {
                                println!("got line = {}", line);
                                Ok(())
                            })
                            .then(move |_| {
                                println!("[CONN{}] [5] response read from server", conn_counter);
                                // send a msg back to client
                                // this prevents the client connection to hang in TIME_WAIT
                                let client_reunited = client_sink
                                    .reunite(client_stream_inner)
                                    .unwrap()
                                    .into_inner();
                                let pong = tokio_io::io::write_all(client_reunited, b"OK\n");
                                tokio::spawn(pong.map_err(|err| eprintln!("{}", err)).then(
                                    move |res| {
                                        let (stream, msg) = res.unwrap();
                                        println!(
                                            "[CONN{}] [6] '{}' msg replied to client {:?}",
                                            conn_counter,
                                            String::from_utf8(msg.to_vec()).unwrap().trim(),
                                            stream
                                        );
                                        Ok(())
                                    },
                                ))
                            });
                        tokio::spawn(line_iter)
                    });
                    tokio::spawn(response_reader)
                });
                tokio::spawn(connected)
            })
            .then(move |result| {
                println!("[CONN{}] [7] client payload read", conn_counter);
                result
            });
        tokio::spawn(read_payload)
    });
    tokio::run(process_reqs.map_err(|_| {}));
}
