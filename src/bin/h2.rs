extern crate bytes;
extern crate env_logger;
extern crate futures;
extern crate h2;
extern crate http;
extern crate tokio;

use h2::server;

use bytes::*;
use futures::*;
use http::*;

use h2::client;
use h2::RecvStream;
use std::thread;
use tokio::executor::current_thread::CurrentThread;
use tokio::net::TcpListener;
use tokio::net::TcpStream;

struct Process {
    body: RecvStream,
    trailers: bool,
}

impl Future for Process {
    type Item = ();
    type Error = h2::Error;

    fn poll(&mut self) -> Poll<(), h2::Error> {
        loop {
            if self.trailers {
                let trailers = try_ready!(self.body.poll_trailers());

                println!("GOT TRAILERS: {:?}", trailers);

                return Ok(().into());
            } else {
                match try_ready!(self.body.poll()) {
                    Some(chunk) => {
                        println!("GOT CHUNK = {:?}", chunk);
                    }
                    None => {
                        self.trailers = true;
                    }
                }
            }
        }
    }
}

fn main() {
    env_logger::init();

    spawn_server();

    let mut exec = CurrentThread::new();

    let tcp = TcpStream::connect(&"127.0.0.1:5928".parse().unwrap())
        .wait()
        .unwrap();

    println!("TCP connected");

    let (mut client, h2) = client::handshake(tcp).wait().unwrap();

    println!("1");

    exec.spawn(h2.map_err(|e| println!("GOT ERR={:?}", e)));

    println!("client connected");

    let mut i = 0;

    loop {
        let request = Request::builder()
            .uri("https://localhost:8080/")
            .body(())
            .unwrap();

        let (response, _stream) = client.send_request(request, true).unwrap();

        let run = response
            .and_then(|response| {
                println!("GOT RESPONSE: {:?}", response);

                // Get the body
                let (_, body) = response.into_parts();

                Process {
                    body,
                    trailers: false,
                }
            }).map_err(|e| {
                println!("GOT ERR={:?}", e);
            });

        exec.block_on(run).unwrap();
        i += 1;
        println!("{}", i);
    }
}

fn spawn_server() {
    let listener = TcpListener::bind(&"127.0.0.1:5928".parse().unwrap()).unwrap();
    println!("listening on {:?}", listener.local_addr().unwrap());
    let server = listener
        .incoming()
        .for_each(move |socket| {
            // let socket = io_dump::Dump::to_stdout(socket);

            let connection = server::handshake(socket)
                .and_then(|conn| {
                    println!("H2 connection bound");

                    conn.for_each(|(request, mut respond)| {
                        println!("GOT request: {:?}", request);

                        let response = Response::builder().status(StatusCode::OK).body(()).unwrap();

                        let mut send = match respond.send_response(response, false) {
                            Ok(send) => send,
                            Err(e) => {
                                println!(" error respond; err={:?}", e);
                                return Ok(());
                            }
                        };

                        println!(">>>> sending data");
                        if let Err(e) = send.send_data(Bytes::from_static(b"hello world"), true) {
                            println!("  -> err={:?}", e);
                        }

                        Ok(())
                    })
                }).and_then(|_| {
                    println!("~~~~~~~~~~~~~~~~~~~~~~~~~~~ H2 connection CLOSE !!!!!! ~~~~~~~~~~~");
                    Ok(())
                }).then(|res| {
                    if let Err(e) = res {
                        println!("  -> err={:?}", e);
                    }

                    Ok(())
                });

            tokio::spawn(Box::new(connection));
            Ok(())
        }).map_err(|e| eprintln!("accept error: {}", e));

    thread::spawn(|| {
        tokio::run(server);
    });
}
