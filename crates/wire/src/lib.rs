//! The relay's envelope: how clients address frames, and how the relay
//! delivers them.
//!
//! This crate exists so the format is defined once. The relay and the client
//! core both depend on it; neither has its own copy. It has no dependencies
//! worth the name, because the relay should not pull in MLS to learn how to
//! route.
//!
//! The relay parses this layer — it has to, in order to route — and never
//! looks past it. Everything after the envelope is a payload the relay moves
//! without reading.
//!
//! # Format
//!
//! Every message starts with a tag byte. Addresses are length-prefixed with a
//! single byte, so they are 1 to 255 bytes long. Integers are big-endian.
//!
//! ```text
//! client → relay
//!   Subscribe   0x01  count:u8  (len:u8 address)*count
//!   Publish     0x02  len:u8 address  payload…
//!
//! relay → client
//!   Delivery    0x01  len:u8 address  seq:u64  accepted_ms:u64  payload…
//! ```
//!
//! Relay → client messages carry a tag even though there is only one kind
//! today, so that a later notice (a client's cursor falling below the
//! retention floor, say) is an addition rather than a format change.

/// Anything that can be wrong with an envelope.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    /// The message had no bytes at all.
    #[error("empty message")]
    Empty,
    /// The leading tag is not one this version knows.
    #[error("unknown message tag {0}")]
    UnknownTag(u8),
    /// The message ended partway through a field.
    #[error("message truncated")]
    Truncated,
    /// An address was empty or longer than [`MAX_ADDRESS_LEN`].
    #[error("address must be 1 to {MAX_ADDRESS_LEN} bytes, got {0}")]
    AddressLength(usize),
    /// A subscribe listed more addresses than fit its count byte.
    #[error("at most {max} addresses per subscribe, got {0}", max = u8::MAX)]
    TooManyAddresses(usize),
}

/// Convenience alias for this crate's fallible operations.
pub type Result<T> = std::result::Result<T, Error>;

/// The longest an address can be: its length is carried in one byte.
pub const MAX_ADDRESS_LEN: usize = u8::MAX as usize;

/// Where a frame goes. Opaque to the relay: a channel's MLS group id and a
/// joiner's `KeyPackageRef` look the same to it, and that is the point.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Address(Vec<u8>);

impl Address {
    /// Wrap raw address bytes.
    ///
    /// # Errors
    /// If `bytes` is empty or longer than [`MAX_ADDRESS_LEN`].
    pub fn new(bytes: Vec<u8>) -> Result<Self> {
        if bytes.is_empty() || bytes.len() > MAX_ADDRESS_LEN {
            return Err(Error::AddressLength(bytes.len()));
        }
        Ok(Self(bytes))
    }

    /// The address as bytes.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// Consume the address, returning its bytes.
    #[must_use]
    pub fn into_bytes(self) -> Vec<u8> {
        self.0
    }

    fn encode_into(&self, out: &mut Vec<u8>) {
        // Cannot truncate: `new` bounds the length to what fits a byte.
        out.push(u8::try_from(self.0.len()).unwrap_or(u8::MAX));
        out.extend_from_slice(&self.0);
    }
}

/// What a client asks of the relay.
#[derive(Debug, PartialEq, Eq)]
pub enum Request {
    /// Start receiving whatever is published to these addresses. Adds to any
    /// earlier subscriptions; there is no unsubscribe short of disconnecting.
    Subscribe(Vec<Address>),
    /// Send a payload to everyone subscribed to `to`. The sender does not
    /// have to be subscribed itself, which is how a joiner reaches a channel
    /// it is not yet in.
    Publish {
        /// Where the payload goes.
        to: Address,
        /// Opaque to the relay.
        payload: Vec<u8>,
    },
}

const SUBSCRIBE: u8 = 0x01;
const PUBLISH: u8 = 0x02;
const DELIVERY: u8 = 0x01;

impl Request {
    /// Serialise for the wire.
    ///
    /// # Errors
    /// If a subscribe lists more than 255 addresses.
    pub fn encode(&self) -> Result<Vec<u8>> {
        match self {
            Self::Subscribe(addresses) => {
                let count = u8::try_from(addresses.len())
                    .map_err(|_| Error::TooManyAddresses(addresses.len()))?;
                let mut out = vec![SUBSCRIBE, count];
                for address in addresses {
                    address.encode_into(&mut out);
                }
                Ok(out)
            }
            Self::Publish { to, payload } => {
                let mut out = Vec::with_capacity(2 + to.0.len() + payload.len());
                out.push(PUBLISH);
                to.encode_into(&mut out);
                out.extend_from_slice(payload);
                Ok(out)
            }
        }
    }

    /// Parse a request off the wire. The input is hostile: any shape of
    /// rubbish must come back as an error, never a panic.
    ///
    /// # Errors
    /// If the message is empty, truncated, or carries an unknown tag or an
    /// invalid address.
    pub fn decode(wire: &[u8]) -> Result<Self> {
        let mut r = Reader(wire);
        match r.byte().map_err(|_| Error::Empty)? {
            SUBSCRIBE => {
                let count = r.byte()?;
                let addresses = (0..count).map(|_| r.address()).collect::<Result<_>>()?;
                Ok(Self::Subscribe(addresses))
            }
            PUBLISH => {
                let to = r.address()?;
                Ok(Self::Publish {
                    to,
                    payload: r.rest(),
                })
            }
            tag => Err(Error::UnknownTag(tag)),
        }
    }
}

/// A payload as the relay hands it to a subscriber.
#[derive(Debug, PartialEq, Eq)]
pub struct Delivery {
    /// The address it was published to.
    pub address: Address,
    /// Its position among everything published to that address. Monotonic
    /// per address, and the same for every subscriber, which is what decides
    /// competing commits.
    pub seq: u64,
    /// When the relay accepted it, in milliseconds since the Unix epoch. For
    /// display ordering only; delivery order is `seq`.
    pub accepted_ms: u64,
    /// Opaque to the relay.
    pub payload: Vec<u8>,
}

impl Delivery {
    /// Serialise for the wire.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(18 + self.address.0.len() + self.payload.len());
        out.push(DELIVERY);
        self.address.encode_into(&mut out);
        out.extend_from_slice(&self.seq.to_be_bytes());
        out.extend_from_slice(&self.accepted_ms.to_be_bytes());
        out.extend_from_slice(&self.payload);
        out
    }

    /// Parse a delivery off the wire.
    ///
    /// # Errors
    /// If the message is empty, truncated, or carries an unknown tag or an
    /// invalid address.
    pub fn decode(wire: &[u8]) -> Result<Self> {
        let mut r = Reader(wire);
        match r.byte().map_err(|_| Error::Empty)? {
            DELIVERY => Ok(Self {
                address: r.address()?,
                seq: r.u64()?,
                accepted_ms: r.u64()?,
                payload: r.rest(),
            }),
            tag => Err(Error::UnknownTag(tag)),
        }
    }
}

/// A cursor over untrusted bytes. Every read is bounds-checked.
struct Reader<'a>(&'a [u8]);

impl Reader<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8]> {
        if self.0.len() < n {
            return Err(Error::Truncated);
        }
        let (head, tail) = self.0.split_at(n);
        self.0 = tail;
        Ok(head)
    }

    fn byte(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }

    fn u64(&mut self) -> Result<u64> {
        let bytes = self.take(8)?;
        let mut buf = [0; 8];
        buf.copy_from_slice(bytes);
        Ok(u64::from_be_bytes(buf))
    }

    fn address(&mut self) -> Result<Address> {
        let len = usize::from(self.byte()?);
        Address::new(self.take(len)?.to_vec())
    }

    fn rest(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.0).to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn addr(bytes: &[u8]) -> Address {
        Address::new(bytes.to_vec()).unwrap()
    }

    #[test]
    fn requests_round_trip() {
        for request in [
            Request::Subscribe(vec![addr(b"channel"), addr(&[7; 32])]),
            Request::Subscribe(vec![]),
            Request::Publish {
                to: addr(b"channel"),
                payload: b"opaque".to_vec(),
            },
            Request::Publish {
                to: addr(&[1; MAX_ADDRESS_LEN]),
                payload: vec![],
            },
        ] {
            let wire = request.encode().unwrap();
            assert_eq!(Request::decode(&wire).unwrap(), request);
        }
    }

    #[test]
    fn deliveries_round_trip() {
        let delivery = Delivery {
            address: addr(&[9; 16]),
            seq: u64::MAX - 1,
            accepted_ms: 1_790_000_000_000,
            payload: b"\x04ciphertext".to_vec(),
        };
        assert_eq!(Delivery::decode(&delivery.encode()).unwrap(), delivery);
    }

    /// Both directions come off the network, so every truncation of a valid
    /// message must be an error rather than a panic or a wrong parse.
    #[test]
    fn every_truncation_is_rejected() {
        let publish = Request::Publish {
            to: addr(b"abc"),
            payload: vec![],
        }
        .encode()
        .unwrap();
        let subscribe = Request::Subscribe(vec![addr(b"abc"), addr(b"de")])
            .encode()
            .unwrap();
        for wire in [&publish, &subscribe] {
            for cut in 0..wire.len() {
                assert!(Request::decode(&wire[..cut]).is_err(), "cut at {cut}");
            }
        }

        let delivery = Delivery {
            address: addr(b"abc"),
            seq: 1,
            accepted_ms: 2,
            payload: vec![],
        }
        .encode();
        for cut in 0..delivery.len() {
            assert!(Delivery::decode(&delivery[..cut]).is_err(), "cut at {cut}");
        }
    }

    #[test]
    fn hostile_shapes_are_errors() {
        assert_eq!(Request::decode(&[]), Err(Error::Empty));
        assert_eq!(Request::decode(&[0x7f]), Err(Error::UnknownTag(0x7f)));
        // A zero-length address is not an address.
        assert_eq!(Request::decode(&[PUBLISH, 0]), Err(Error::AddressLength(0)));
        // A count promising more addresses than follow.
        assert_eq!(
            Request::decode(&[SUBSCRIBE, 3, 1, b'a']),
            Err(Error::Truncated)
        );
        assert_eq!(Delivery::decode(&[0x02]), Err(Error::UnknownTag(0x02)));
    }

    #[test]
    fn address_bounds_are_enforced() {
        assert_eq!(Address::new(vec![]), Err(Error::AddressLength(0)));
        assert_eq!(
            Address::new(vec![0; MAX_ADDRESS_LEN + 1]),
            Err(Error::AddressLength(MAX_ADDRESS_LEN + 1))
        );
        let many = Request::Subscribe(vec![addr(b"x"); 256]);
        assert_eq!(many.encode(), Err(Error::TooManyAddresses(256)));
    }
}
