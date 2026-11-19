//! Loopback host for the three files in a release browser bundle.
use std::{fs, net::TcpListener, path::Path};

pub(super) fn serve(site: &Path, port: u16) -> Result<(), String> {
    for (file, _) in ASSETS {
        fs::File::open(site.join(file))
            .map_err(|e| format!("{file}: {e} (run cargo xtask web-build first)"))?;
    }
    let listener = TcpListener::bind(("127.0.0.1", port)).map_err(|e| e.to_string())?;
    println!(
        "Serving {} at http://{}/",
        site.display(),
        listener.local_addr().map_err(|e| e.to_string())?
    );
    for connection in listener.incoming() {
        let stream = match connection {
            Ok(stream) => stream,
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::Interrupted | std::io::ErrorKind::ConnectionAborted
                ) =>
            {
                continue;
            }
            Err(error) => return Err(error.to_string()),
        };
        if let Err(error) = reply(stream, site) {
            eprintln!("Preview request: {error}");
        }
    }
    Ok(())
}

const ASSETS: [(&str, &str); 3] = [
    ("index.html", "text/html; charset=utf-8"),
    ("pkg/histeries_client.js", "text/javascript"),
    ("pkg/histeries_client_bg.wasm", "application/wasm"),
];

fn reply(mut stream: std::net::TcpStream, site: &Path) -> std::io::Result<()> {
    use std::io::{self, BufRead, BufReader, Read, Write};
    use std::time::Duration;

    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    stream.set_write_timeout(Some(Duration::from_secs(10)))?;
    let mut request = String::new();
    BufReader::new(&stream).take(8192).read_line(&mut request)?;
    let mut fields = request.split_whitespace();
    let method = fields.next().unwrap_or_default();
    let path = fields
        .next()
        .unwrap_or_default()
        .split('?')
        .next()
        .unwrap_or_default();
    let name = if path == "/" {
        Some("index.html")
    } else {
        path.strip_prefix('/')
    };
    let asset = ASSETS.iter().find(|(file, _)| Some(*file) == name);
    if method != "GET" && method != "HEAD" {
        return stream.write_all(b"HTTP/1.1 405 Method Not Allowed\r\nAllow: GET, HEAD\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
    }
    let Some((file, mime)) = asset else {
        return stream.write_all(
            b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        );
    };
    let mut file = fs::File::open(site.join(file))?;
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: {mime}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        file.metadata()?.len()
    )?;
    if method == "GET" {
        io::copy(&mut file, &mut stream)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::reply;
    use std::{
        io::{Read, Write},
        net::{TcpListener, TcpStream},
        thread,
    };

    fn request(line: &str) -> String {
        let site = crate::workspace_root().join("web");
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let address = listener.local_addr().unwrap();
        thread::scope(|scope| {
            let server = scope.spawn(|| reply(listener.accept().unwrap().0, &site));
            let mut stream = TcpStream::connect(address).unwrap();
            write!(stream, "{line} HTTP/1.1\r\nHost: localhost\r\n\r\n").unwrap();
            let mut response = String::new();
            stream.read_to_string(&mut response).unwrap();
            server.join().unwrap().unwrap();
            response
        })
    }

    #[test]
    fn preview_serves_the_html_and_limits_paths_and_methods() {
        let get = request("GET /?preview");
        assert!(get.starts_with("HTTP/1.1 200 OK"));
        assert!(get.contains("Content-Type: text/html"));
        assert!(get.contains("<canvas"));
        let head = request("HEAD /index.html");
        assert!(head.starts_with("HTTP/1.1 200 OK"));
        assert!(head.ends_with("\r\n\r\n"));
        assert!(request("GET /Cargo.toml").starts_with("HTTP/1.1 404"));
        assert!(request("POST /").starts_with("HTTP/1.1 405"));
    }
}
