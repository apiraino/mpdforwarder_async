extern crate encoding;
extern crate tokio;

// use std::io::{BufReader, BufWriter};

use tokio::prelude::*;
use tokio::io;
use tokio::net::{TcpListener, TcpStream};

use encoding::{EncoderTrap, Encoding};
use encoding::all::ASCII;

fn main() {
    // Bind the server's socket.
    let addr = "127.0.0.1:6601".parse().unwrap();
    let listener = TcpListener::bind(&addr).expect("unable to bind TCP listener");
    println!("Server listening on tcp://{}", addr);

    // Pull out a stream of sockets for incoming connections
    let server = listener
        .incoming()
        .map_err(|e| eprintln!("accept failed = {:?}", e))
        .for_each(|sock| {
            println!("accepted socket; addr={:?}", sock.peer_addr().unwrap());
            // Split up the reading and writing parts of the
            // socket.
            // let (reader, writer) = sock.split();

            // A future that echos the data and returns how
            // many bytes were copied...
            // let handle_conn = io::copy(reader, writer)
            //     .map(|amt| println!("wrote {:?} bytes", amt))
            //     .map_err(|err| eprintln!("IO error {:?}", err))
            //     .then(|res| {
            //         println!("wrote message; success={:?}", res.is_ok());
            //         Ok(())
            //     });

            // let handle_conn = io::write_all(sock, "hello world\n").then(|res| {
            //     println!("wrote message; success={:?}", res);
            //     Ok(())
            // });

            // let buf = vec![0; 4];
            // let handle_conn = io::read_to_end(sock, buf).then(|res| {
            //     println!("Got: '{:?}'", res);
            //     Ok(())
            // });

            // let (reader, _writer) = sock.split();
            let buf = vec![];
            let handle_conn = io::read_to_end(sock, buf).then(|res| {
                // res ==> std::result::Result<(tokio::net::TcpStream, std::vec::Vec<u8>), std::io::Error>
                match res {
                    Err(_) => println!("Error"),
                    Ok((_tcp_stream, payload)) => {
                        // println!("payload='{:?}'", payload);
                        let mut payload_str = String::from_utf8(payload).unwrap();
                        payload_str = payload_str.trim().to_string();
                        // println!("payload_str='{}' ({})", payload_str, payload_str.len());
                        let addr2 = "127.0.0.1:6600".parse().unwrap();
                        let connect_to_dest = TcpStream::connect(&addr2).and(move |res2| {
                            println!("answer={:?}", res2);

                            let mut read_response = vec![0, 13 + 1];
                            res2.read_exact(&mut read_response);

                            match res2 {
                                Err(err2) => println!("Error: {}", err2),
                                Ok(mut stream) => {
                                    // let mut reader = BufReader::new(&stream);
                                    // let mut writer = BufWriter::new(&stream);

                                    // tokio_io::split::ReadHalf<tokio::net::TcpStream>
                                    // tokio_io::split::WriteHalf<tokio::net::TcpStream>
                                    let (mut reader, mut writer) = stream.split();

                                    // let mut response = vec![0; 13 + 1];
                                    // stream.read_exact(&mut response).unwrap();
                                    // println!(
                                    //     "[connect] response='{:?}'",
                                    //     String::from_utf8(response).unwrap()
                                    // );

                                    // let mut command_bytes =
                                    //     ASCII.encode(&payload_str, EncoderTrap::Strict).unwrap();
                                    // command_bytes.push('\r' as u8);
                                    // command_bytes.push('\n' as u8);

                                    // println!("pushing payload = '{:?}'", command_bytes);
                                    // wtf: '[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 97, 97, 97, 13, 10]'
                                    // ^^^ due to: let buf = vec![0,10];

                                    // stream.write_all(&command_bytes).unwrap();
                                    // writer.write(&command_bytes).unwrap();
                                    writer.write(b"stop").unwrap();

                                    let mut response = vec![0; 13 + 1];
                                    reader.read_exact(&mut response).unwrap();
                                    println!(
                                        "[cmd] response='{:?}'",
                                        String::from_utf8(response).unwrap()
                                    );

                                    // write!(stream, "{:.*}\r\n", payload_str.len() + 2, payload_str)
                                    //     .unwrap();
                                    // writer.flush().unwrap();

                                    // write!(stream, "{:.*}\r\n", 4, payload_str).unwrap()

                                    // stream.write(&command_bytes).unwrap();
                                }
                            };
                            Ok(())
                        });
                        tokio::spawn(connect_to_dest);
                        // tokio::spawn(connect_to_dest).map_err(|err| println!("{:?}", err)));
                    }
                };

                Ok(())
            });

            // Spawn the future as a concurrent task.
            tokio::spawn(handle_conn);

            Ok(())
        });

    // Start the Tokio runtime
    tokio::run(server);
}
