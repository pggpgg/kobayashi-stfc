use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};

pub mod api;
pub mod routes;
pub mod sync;

const MAX_HEADER_SIZE: usize = 8 * 1024;
const MAX_BODY_SIZE: usize = 1024 * 1024;

pub fn run_server(bind_addr: &str) -> std::io::Result<()> {
    let listener = TcpListener::bind(bind_addr)?;
    println!("kobayashi server listening on http://{bind_addr}");

    for stream in listener.incoming() {
        match stream {
            Ok(mut stream) => {
                if let Err(err) = handle_connection(&mut stream) {
                    eprintln!("request error: {err}");
                }
            }
            Err(err) => eprintln!("connection failed: {err}"),
        }
    }

    Ok(())
}

fn handle_connection(stream: &mut TcpStream) -> std::io::Result<()> {
    let response = match read_http_request(stream) {
        Ok(request) => routes::route_request(
            &request.method,
            &request.path,
            &request.body,
            request.sync_token.as_deref(),
        ),
        Err(ParseError::BadRequest(message)) => {
            transport_error_response(400, "Bad Request", message)
        }
        Err(ParseError::PayloadTooLarge(message)) => {
            transport_error_response(413, "Payload Too Large", message)
        }
        Err(ParseError::Io(err)) => return Err(err),
    }
    .to_http_string();

    stream.write_all(response.as_bytes())?;
    stream.flush()?;
    Ok(())
}

#[derive(Debug)]
struct ParsedRequest {
    method: String,
    path: String,
    body: String,
    /// Value of the stfc-sync-token request header, if present (for sync ingress auth).
    sync_token: Option<String>,
}

#[derive(Debug)]
enum ParseError {
    BadRequest(&'static str),
    PayloadTooLarge(&'static str),
    Io(std::io::Error),
}

impl From<std::io::Error> for ParseError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err)
    }
}

fn read_http_request<R: Read>(reader: &mut R) -> Result<ParsedRequest, ParseError> {
    let mut request_bytes = Vec::with_capacity(1024);
    let header_end = loop {
        if let Some(index) = find_header_terminator(&request_bytes) {
            break index;
        }
        if request_bytes.len() >= MAX_HEADER_SIZE {
            return Err(ParseError::PayloadTooLarge(
                "Request headers exceed maximum allowed size",
            ));
        }

        let mut chunk = [0_u8; 1024];
        let read_count = reader.read(&mut chunk)?;
        if read_count == 0 {
            return Err(ParseError::BadRequest("Incomplete request headers"));
        }

        request_bytes.extend_from_slice(&chunk[..read_count]);
    };

    let header_bytes = &request_bytes[..header_end];
    let header_text = std::str::from_utf8(header_bytes)
        .map_err(|_| ParseError::BadRequest("Headers must be valid UTF-8"))?;
    let (method, path, content_length, sync_token) = parse_headers(header_text)?;

    let body_len = content_length.unwrap_or(0);
    if body_len > MAX_BODY_SIZE {
        return Err(ParseError::PayloadTooLarge(
            "Request body exceeds maximum allowed size",
        ));
    }

    let method_requires_length = matches!(method.as_str(), "POST" | "PUT" | "PATCH");
    if method_requires_length && content_length.is_none() {
        return Err(ParseError::BadRequest("Missing Content-Length header"));
    }

    let mut body_bytes = request_bytes[(header_end + 4)..].to_vec();
    while body_bytes.len() < body_len {
        let mut chunk = [0_u8; 1024];
        let read_count = reader.read(&mut chunk)?;
        if read_count == 0 {
            return Err(ParseError::BadRequest(
                "Request body shorter than Content-Length",
            ));
        }
        body_bytes.extend_from_slice(&chunk[..read_count]);

        if body_bytes.len() > MAX_BODY_SIZE {
            return Err(ParseError::PayloadTooLarge(
                "Request body exceeds maximum allowed size",
            ));
        }
    }

    let body = String::from_utf8(body_bytes[..body_len].to_vec())
        .map_err(|_| ParseError::BadRequest("Body must be valid UTF-8"))?;

    Ok(ParsedRequest {
        method,
        path,
        body,
        sync_token,
    })
}

fn parse_headers(
    header_text: &str,
) -> Result<(String, String, Option<usize>, Option<String>), ParseError> {
    let mut lines = header_text.split("\r\n");
    let request_line = lines
        .next()
        .ok_or(ParseError::BadRequest("Missing request line"))?;

    let mut request_parts = request_line.split_whitespace();
    let method = request_parts
        .next()
        .ok_or(ParseError::BadRequest("Missing HTTP method"))?;
    let path = request_parts
        .next()
        .ok_or(ParseError::BadRequest("Missing request path"))?;

    let mut content_length = None;
    let mut sync_token = None;
    for line in lines {
        if line.is_empty() {
            break;
        }

        let (name, value) = line
            .split_once(':')
            .ok_or(ParseError::BadRequest("Malformed header line"))?;

        if name.eq_ignore_ascii_case("Content-Length") {
            let length = value
                .trim()
                .parse::<usize>()
                .map_err(|_| ParseError::BadRequest("Invalid Content-Length header"))?;
            content_length = Some(length);
        } else if name.eq_ignore_ascii_case("stfc-sync-token") {
            sync_token = Some(value.trim().to_string());
        }
    }

    Ok((
        method.to_string(),
        path.to_string(),
        content_length,
        sync_token,
    ))
}

fn find_header_terminator(buffer: &[u8]) -> Option<usize> {
    buffer.windows(4).position(|window| window == b"\r\n\r\n")
}

fn transport_error_response(
    status_code: u16,
    status_text: &'static str,
    message: &'static str,
) -> routes::HttpResponse {
    routes::HttpResponse {
        status_code,
        status_text,
        content_type: "application/json",
        body: format!(
            "{{\n  \"status\": \"error\",\n  \"message\": {}\n}}",
            serde_json::to_string(message).unwrap_or_else(|_| "\"Unknown error\"".to_string())
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::{read_http_request, ParseError};
    use std::collections::VecDeque;
    use std::io::{self, Read};

    struct ChunkedReader {
        chunks: VecDeque<Vec<u8>>,
    }

    impl ChunkedReader {
        fn from_chunks(chunks: Vec<&[u8]>) -> Self {
            Self {
                chunks: chunks.into_iter().map(|chunk| chunk.to_vec()).collect(),
            }
        }
    }

    impl Read for ChunkedReader {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            let Some(chunk) = self.chunks.front_mut() else {
                return Ok(0);
            };

            let copy_len = chunk.len().min(buf.len());
            buf[..copy_len].copy_from_slice(&chunk[..copy_len]);
            chunk.drain(..copy_len);
            if chunk.is_empty() {
                self.chunks.pop_front();
            }

            Ok(copy_len)
        }
    }

    #[test]
    fn reads_body_across_multiple_reads() {
        let mut reader = ChunkedReader::from_chunks(vec![
            b"POST /api/optimize HTTP/1.1\r\nContent-Length: 18\r\n\r\n{\"ship\":\"Sa",
            b"ladin\"}",
        ]);

        let request = read_http_request(&mut reader).expect("request should parse");
        assert_eq!(request.method, "POST");
        assert_eq!(request.path, "/api/optimize");
        assert_eq!(request.body, "{\"ship\":\"Saladin\"}");
    }

    #[test]
    fn parses_json_payload_spanning_many_tcp_reads() {
        let mut reader = ChunkedReader::from_chunks(vec![
            b"POST /api/optimize HTTP/1.1\r\nCont",
            b"ent-Length: 42\r\n\r\n{\"ship\":\"S",
            b"aladin\",\"hostile\":\"E",
            b"xplorer_30\"}",
        ]);

        let request = read_http_request(&mut reader).expect("request should parse");
        assert_eq!(
            request.body,
            "{\"ship\":\"Saladin\",\"hostile\":\"Explorer_30\"}"
        );
    }

    #[test]
    fn rejects_missing_or_invalid_content_length() {
        let mut missing_length = ChunkedReader::from_chunks(vec![
            b"POST /api/optimize HTTP/1.1\r\nHost: localhost\r\n\r\n{}",
        ]);
        let missing_error = read_http_request(&mut missing_length).expect_err("expected error");
        assert!(matches!(
            missing_error,
            ParseError::BadRequest("Missing Content-Length header")
        ));

        let mut invalid_length = ChunkedReader::from_chunks(vec![
            b"POST /api/optimize HTTP/1.1\r\nContent-Length: nope\r\n\r\n{}",
        ]);
        let invalid_error = read_http_request(&mut invalid_length).expect_err("expected error");
        assert!(matches!(
            invalid_error,
            ParseError::BadRequest("Invalid Content-Length header")
        ));
    }
}
