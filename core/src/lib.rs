use bytes::{BufMut, BytesMut};
use tokio::codec::{Decoder, Encoder};
use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

/// Client request containign the number of random IPv4 addresses it wishes to
/// receive from server.
pub struct AddrRequest {
    num_addrs: u32,
}

/// Server response containing random IPv4 addresses.
pub struct AddrResponse {
    addrs: Vec<SocketAddr>,
}

pub struct ClientToServerCodec;

/// Encoded client request format is as follows:
///
/// <32:n>
///
/// Where n is a 32-bit integer denoting the number of random ipv4 addresses
impl Encoder for ClientToServerCodec {
    type Item = AddrRequest;
    type Error = io::Error;

    fn encode(&mut self, item: AddrRequest, buf: &mut BytesMut) -> io::Result<()> {
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
    type Item = AddrResponse;
    type Error = io::Error;

    fn decode(&mut self, buf: &mut BytesMut) -> io::Result<Option<AddrResponse>> {
        if buf.len() < 4 {
            // Need at least four bytes for the length field.
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
            n as usize
        };
        // Check if we have all addresses in the response which has a 4 byte
        // length field and `num_addr` times 6 bytes (an address containsa
        // 4 byte IP and a 2 byte port).
        if buf.len() < 4 + (num_addrs * 6) {
            return Ok(None)
        }
        // Offset into the buffer.
        let mut offset = 4;
        let mut addrs = Vec::with_capacity(num_addrs);
        for i in 0..num_addrs {
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
                for i in 0..4 {
                    n <<= 8;
                    n |= buf[offset + i] as u16;
                }
                n
            };
            offset += 2;
            addrs.push(SocketAddr::new(ip, port));
        }
        Ok(Some(AddrResponse { addrs }))
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
    type Item = AddrResponse;
    type Error = io::Error;

    fn encode(&mut self, item: AddrResponse, buf: &mut BytesMut) -> io::Result<()> {
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
        Ok(())
    }
}

/// Encoded client request format is as follows:
///
/// <32:n>
///
/// Where n is a 32-bit integer denoting the number of random ipv4 addresses
impl Decoder for ServerToClientCodec {
    type Item = AddrRequest;
    type Error = io::Error;

    fn decode(&mut self, buf: &mut BytesMut) -> io::Result<Option<AddrRequest>> {
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
        // TODO parse addresses
        Ok(Some(AddrRequest { num_addrs }))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        // TODO: add tests
        assert_eq!(2 + 2, 4);
    }
}
