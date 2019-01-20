use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use bytes::{BufMut, BytesMut};

use log::*;
use simplelog::*;

use tokio::codec::{Decoder, Encoder};

/// Client request containign the number of random IPv4 addresses it wishes to
/// receive from server.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Request {
    pub num_addrs: u32,
}

/// Server response containing random IPv4 addresses.
#[derive(Clone, Debug, PartialEq)]
pub struct Response {
    pub addrs: Vec<SocketAddr>,
}

pub struct ClientToServerCodec;

/// Encoded client request format is as follows:
///
/// <32:n>
///
/// Where n is a 32-bit integer denoting the number of random ipv4 addresses
impl Encoder for ClientToServerCodec {
    type Item = Request;
    type Error = io::Error;

    fn encode(&mut self, item: Request, buf: &mut BytesMut) -> io::Result<()> {
        info!("Encoding {:?}", item);
        buf.put_u32_be(item.num_addrs);
        Ok(())
    }
}

/// Encoded server response format is as follows:
///
/// <32:n><<32:ip><16:port>><<32:ip><16:port>>...<<32:ip><16:port>>
///
/// Where n is a 32-bit integer denoting the number of 32-bit IPv4 addresses
/// contained in the response.
impl Decoder for ClientToServerCodec {
    type Item = Response;
    type Error = io::Error;

    fn decode(&mut self, buf: &mut BytesMut) -> io::Result<Option<Response>> {
        if buf.len() < 4 {
            // Need at least four bytes for the length field.
            return Ok(None);
        }
        let payload_len = {
            // Convert from network byte order to host byte order. TODO can't
            // BytesMut take care of this?
            let mut n: u32 = 0;
            for i in 0..4 {
                n <<= 8;
                n |= buf[i] as u32;
            }
            n as usize
        };
        if payload_len % 6 != 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Invalid payload length"
            ));
        }
        let num_addrs = payload_len / 6;
        info!("#addrs: {}", num_addrs);
        // Check if we have all addresses in the response which has a 4 byte
        // length field and `num_addrs` times 6 bytes (an address containsa
        // 4 byte IP and a 2 byte port).
        let msg_len = 4 + payload_len;
        if buf.len() < msg_len {
            return Ok(None)
        }
        info!("msg len: {}", msg_len);
        // Start offset into the buffer at 4 to skip initial length field.
        let mut offset = 4;
        let mut addrs = Vec::with_capacity(num_addrs);
        for _ in 0..num_addrs {
            let ip = IpAddr::V4(Ipv4Addr::new(
                    buf[offset],
                    buf[offset + 1],
                    buf[offset + 2],
                    buf[offset + 3],
            ));
            //TODO let ip = IpAddr::V4(Ipv4Addr::from(&buf[offset..offset+4]));
            offset += 4;
            let port = {
                let mut n: u16 = 0;
                for i in 0..2 {
                    n <<= 8;
                    n |= buf[offset + i] as u16;
                }
                n
            };
            offset += 2;
            addrs.push(SocketAddr::new(ip, port));
        }
        buf.split_to(msg_len);
        Ok(Some(Response { addrs }))
    }
}

pub struct ServerToClientCodec;

/// Encoded server response format is as follows:
///
/// <32:n><<32:ip><16:port>><<32:ip><16:port>>...<<32:ip><16:port>>
///
/// Where n is a 32-bit integer denoting the number of 32-bit IPv4 addresses
/// contained in the response.
impl Encoder for ServerToClientCodec {
    type Item = Response;
    type Error = io::Error;

    fn encode(&mut self, item: Response, buf: &mut BytesMut) -> io::Result<()> {
        info!("Encoding {:?}", item);
        // TODO: test that item.len() <= 32?
        buf.put_u32_be(item.addrs.len() as u32 * 6);
        for addr in item.addrs {
            let ip = match addr.ip() {
                IpAddr::V4(ip) => ip,
                _ => return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Only IPv4 supported"
                )),
            };
            buf.extend_from_slice(&ip.octets());
            buf.put_u16_be(addr.port());
        }
        info!("Encoded: {:?}", buf);
        Ok(())
    }
}

/// Encoded client request format is as follows:
///
/// <32:n>
///
/// Where n is a 32-bit integer denoting the number of random ipv4 addresses
impl Decoder for ServerToClientCodec {
    type Item = Request;
    type Error = io::Error;

    fn decode(&mut self, buf: &mut BytesMut) -> io::Result<Option<Request>> {
        if buf.len() < 4 {
            // Not enough bytes yet.
            return Ok(None);
        }
        let num_addrs = {
            // Convert from network byte order to host byte order. TODO can't
            // BytesMut take care of this?
            let mut n: u32 = 0;
            for i in 0..4 {
                n <<= 8;
                n |= buf[i] as u32;
            }
            n
        };
        buf.split_to(4);
        Ok(Some(Request { num_addrs }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_to_server_request() {
        let mut buf = BytesMut::with_capacity(1024);
        let req = Request { num_addrs: 5 };
        ClientToServerCodec.encode(req, &mut buf);

        let mut expected_buf = BytesMut::with_capacity(1024);
        expected_buf.put_u32_be(5);
        assert_eq!(&buf[..4], &expected_buf[..4]);
    }

    #[test]
    fn client_to_server_response() {
        let msg_len = 4 + 2 * 6;

        let mut buf = BytesMut::with_capacity(1024);
        buf.put_u32_be(2 * 6);
        buf.put_u8(0);
        buf.put_u8(1);
        buf.put_u8(2);
        buf.put_u8(3);
        buf.put_u16_be(16222);
        buf.put_u8(255);
        buf.put_u8(1);
        buf.put_u8(5);
        buf.put_u8(22);
        buf.put_u16_be(5888);

        let expected_resp = Response {
            addrs: vec![
                SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 1, 2, 3)), 16222),
                SocketAddr::new(IpAddr::V4(Ipv4Addr::new(255, 1, 5, 22)), 5888),
            ],
        };
        match ClientToServerCodec.decode(&mut buf) {
            Ok(Some(resp)) => assert_eq!(resp, expected_resp),
            _ => assert!(false),
        };
    }

    #[test]
    fn server_to_client_request() {
        let mut buf = BytesMut::with_capacity(1024);
        buf.put_slice(&[0, 0, 0, 5]);
        match ServerToClientCodec.decode(&mut buf) {
            Ok(Some(req)) => assert_eq!(req, Request { num_addrs: 5 }),
            _ => assert!(false),
        }
    }

    #[test]
    fn server_to_client_response() {
        let mut buf = BytesMut::with_capacity(1024);
        let resp = Response {
            addrs: vec![
                SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 1, 2, 3)), 16222),
                SocketAddr::new(IpAddr::V4(Ipv4Addr::new(255, 1, 5, 22)), 5888),
            ],
        };
        ServerToClientCodec.encode(resp, &mut buf);

        let msg_len = 4 + 2 * 6;

        let mut expected_buf = BytesMut::with_capacity(1024);
        expected_buf.put_u32_be(2 * 6);
        expected_buf.put_u8(0);
        expected_buf.put_u8(1);
        expected_buf.put_u8(2);
        expected_buf.put_u8(3);
        expected_buf.put_u16_be(16222);
        expected_buf.put_u8(255);
        expected_buf.put_u8(1);
        expected_buf.put_u8(5);
        expected_buf.put_u8(22);
        expected_buf.put_u16_be(5888);
        assert_eq!(&buf[..msg_len], &expected_buf[..msg_len]);
    }
}
