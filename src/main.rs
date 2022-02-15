use nix::{
    sys::{
        signal::{kill, Signal},
        socket::{
            accept, bind, listen, recv, setsockopt, socket, sockopt, AddressFamily, InetAddr,
            IpAddr, MsgFlags, SockAddr, SockFlag, SockType,
        },
        wait::wait,
    },
    unistd::{close, fork, write},
};
use std::{fs, path::Path};

fn do_dir(url: &str) -> Vec<u8> {
    if let Ok(p) = fs::read_dir(url) {
        let mut paths = String::new();
        for di in p {
            if let Ok(de) = di {
                let s = de.path().to_str().unwrap().to_string();
                paths.push_str(&format!("<a href='{s}'>{s}</a><br/>"));
            }
        }
        let content = format!(
            r"
        <html>
        <body>
        {paths}
        </body>
        </html>
        "
        )
        .as_bytes()
        .to_vec();

        let s = content.len();
        let header = format!("HTTP/1.1 200 OK\nContent-Type: text/html\nContent-Length: {s}\n\n").as_bytes().to_vec();
        let mut res = Vec::with_capacity(header.len() + s);
        res.extend(header);
        res.extend(content);
        res
    } else {
        "HTTP/1.1 404 NOT FOUND\nContent-Type: text/plain\nContent-Length: 0\n"
            .as_bytes()
            .to_vec()
    }
}

fn do_file(url: &str) -> Vec<u8> {
    if let Ok(content) = fs::read(&url) {
        let s = content.len();
        let mime = mime_guess::from_path(url)
            .first_or_octet_stream();
        let mime_essence = mime.essence_str();
        let header =
            format!("HTTP/1.1 200 OK\nContent-Type: {mime_essence}\nContent-Length: {s}\n\n").as_bytes().to_vec();
        let mut res = Vec::with_capacity(header.len() + s);
        res.extend(header);
        res.extend(content);
        res
    } else {
        "HTTP/1.1 404 NOT FOUND\nContent-Type: text/plain\nContent-Length: 0\n"
            .as_bytes()
            .to_vec()
    }
}

fn handle_request(req: &str) -> Vec<u8> {
    if req.starts_with("GET") {
        if let Some(i) = req[4..].find(' ') {
            let url = &req[4..4 + i];
            if url == "/" {
                do_dir(".")
            } else {
                if Path::new(&url[1..]).is_dir() {
                    do_dir(&url[1..])
                } else {
                    do_file(&url[1..])
                }
            }
        } else {
            "HTTP/1.1 400 BAD REQUEST\nContent-Type: text/plain\nContent-Length: 0\n"
                .as_bytes()
                .to_vec()
        }
    } else {
        "HTTP/1.1 400 BAD REQUEST\nContent-Type: text/plain\nContent-Length: 0\n"
            .as_bytes()
            .to_vec()
    }
}

fn run_listener() -> Result<(), &'static str> {
    let sock = socket(
        AddressFamily::Inet,
        SockType::Stream,
        SockFlag::empty(),
        None,
    )
    .map_err(|_| "Unable to create socket :(")?;
    setsockopt(sock, sockopt::ReusePort, &true).map_err(|_| "Unable to set socket opts")?;
    bind(
        sock,
        &SockAddr::Inet(InetAddr::new(IpAddr::new_v4(127, 0, 0, 1), 42069)),
    )
    .map_err(|_| "Unable to bind socket")?;
    listen(sock, 128).map_err(|_| "Unable to listen")?;

    let mut buf = [0u8; 1024];
    loop {
        match accept(sock) {
            Ok(client_sock) => {
                if let Ok(sz) = recv(client_sock, &mut buf, MsgFlags::empty()) {
                    match std::str::from_utf8(&buf[0..sz]) {
                        Ok(req) => {
                            let _ = write(client_sock, &handle_request(req));
                        }
                        Err(_) => {}
                    }
                }
                let _ = close(client_sock);
            }
            Err(_) => {}
        }
    }
}

fn main() -> Result<(), &'static str> {
    println!("Starting...");

    let n = num_cpus::get();
    let mut children = vec![];

    if n > 1 {
        for _ in 0..n {
            match unsafe { fork() } {
                Ok(nix::unistd::ForkResult::Parent { child }) => {
                    children.push(child);
                }
                Ok(nix::unistd::ForkResult::Child) => {
                    run_listener()?;
                }
                _ => {}
            }
        }

        match wait() {
            Ok(_) => {
                for child in children {
                    if matches!(kill(child, Signal::SIGTERM), Err(_)) {
                        println!("Unable to kill child process {child}");
                    }
                }
                println!("Exiting");
                Ok(())
            }
            Err(errno) => {
                println!("No child processes. {errno}");
                Err("Something went wrong")
            }
        }
    } else {
        run_listener()?;
        Ok(())
    }
}
