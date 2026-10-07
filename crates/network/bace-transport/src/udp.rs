use bace_wire::{CLIENT_DATAGRAM_LIMIT, SERVER_DATAGRAM_LIMIT};
use std::io;
use std::net::{SocketAddr, UdpSocket};

/// ACE's paired listener ports. The initial reply uses Client; after the
/// companion endpoint is established, outgoing traffic uses Server.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EndpointKind {
    Client,
    Server,
}

/// Both nonblocking UDP sockets, owned by one network worker. No thread,
/// clock, per-peer tasks, authentication or world state is hidden here.
pub struct UdpEndpoints {
    client: UdpSocket,
    server: UdpSocket,
    client_address: SocketAddr,
    server_address: SocketAddr,
}
impl UdpEndpoints {
    /// Binds both sockets atomically from the caller's perspective: failure to
    /// bind/configure either drops both. Base port zero selects an ephemeral
    /// first port, then attempts its adjacent port.
    pub fn bind(base: SocketAddr) -> io::Result<Self> {
        let client = UdpSocket::bind(base)?;
        let client_address = client.local_addr()?;
        let second_port = client_address.port().checked_add(1).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "UDP base port has no companion port",
            )
        })?;
        let server = UdpSocket::bind(SocketAddr::new(client_address.ip(), second_port))?;
        let server_address = server.local_addr()?;
        client.set_nonblocking(true)?;
        server.set_nonblocking(true)?;
        Ok(Self {
            client,
            server,
            client_address,
            server_address,
        })
    }
    pub fn local_address(&self, endpoint: EndpointKind) -> SocketAddr {
        match endpoint {
            EndpointKind::Client => self.client_address,
            EndpointKind::Server => self.server_address,
        }
    }
    fn socket(&self, endpoint: EndpointKind) -> &UdpSocket {
        match endpoint {
            EndpointKind::Client => &self.client,
            EndpointKind::Server => &self.server,
        }
    }
    /// A reusable 1025-byte buffer detects oversized datagrams even where the
    /// OS truncates successfully. Oversized input is consumed and reported as
    /// InvalidData; WouldBlock is None. Only buffer[..length] is initialized
    /// with the received datagram and must be passed to decoding.
    pub fn receive(
        &self,
        endpoint: EndpointKind,
        buffer: &mut [u8; CLIENT_DATAGRAM_LIMIT + 1],
    ) -> io::Result<Option<(SocketAddr, usize)>> {
        match self.socket(endpoint).recv_from(buffer) {
            Ok((length, source)) if length <= CLIENT_DATAGRAM_LIMIT => Ok(Some((source, length))),
            Ok(_) => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "oversized UDP datagram",
            )),
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => Ok(None),
            // Winsock reports truncation as WSAEMSGSIZE instead of success.
            Err(error) if cfg!(windows) && error.raw_os_error() == Some(10040) => Err(
                io::Error::new(io::ErrorKind::InvalidData, "oversized UDP datagram"),
            ),
            Err(error) => Err(error),
        }
    }
    /// A WouldBlock result means the caller must retain this exact datagram in
    /// its bounded output queue. No bytes or reliable messages are discarded.
    pub fn send_to(
        &self,
        endpoint: EndpointKind,
        address: SocketAddr,
        bytes: &[u8],
    ) -> io::Result<()> {
        if bytes.len() > SERVER_DATAGRAM_LIMIT {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "outgoing UDP datagram exceeds ACE limit",
            ));
        }
        let length = self.socket(endpoint).send_to(bytes, address)?;
        if length != bytes.len() {
            return Err(io::Error::new(
                io::ErrorKind::WriteZero,
                "incomplete UDP datagram send",
            ));
        }
        Ok(())
    }
}
