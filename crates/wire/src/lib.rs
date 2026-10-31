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
//!   Resume      0x03  len:u8 address  from:u64
//!
//! relay → client
//!   Delivery    0x01  len:u8 address  seq:u64  accepted_ms:u64  payload…
//!   Page        0x02  len:u8 address  next:u64  live:u8
//!   Unavailable 0x03  len:u8 address  floor:u64  next:u64
//! ```
//!
//! # Catching up
//!
//! A client that has been away sends `Resume` with the first sequence number
//! it has not seen. The relay sends what it has stored from there, a page at
//! a time, each page ending in a `Page`. If `live` is 0 there is more, and the
//! client asks again from `next`. If `live` is 1 the client has everything,
//! and is now subscribed: the relay registered it in the same step as reading
//! the last page, so no frame can fall between catch-up and live delivery.
//! Paging keeps a client back from a month away from arriving as one burst,
//! and keeps the relay free of per-client state between requests.
//!
//! `Unavailable` means the relay cannot serve that cursor. Either retention
//! has removed frames the client never saw (`from` below `floor`), or the
//! client is ahead of everything the relay has (`from` above `next`), which
//! means the relay lost its store. A client that missed a commit cannot read
//! anything after it, so neither case is quietly served from wherever history
//! happens to start.

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
    /// A flag byte was neither 0 nor 1.
    #[error("flag must be 0 or 1, got {0}")]
    InvalidFlag(u8),
    /// Bytes followed the last field of a message that has no payload.
    #[error("{0} unexpected trailing bytes")]
    TrailingBytes(usize),
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
    /// Send what is stored for `address` from sequence `from`, a page at a
    /// time, and subscribe once the client has it all. See the module docs.
    Resume {
        /// The address to catch up on.
        address: Address,
        /// The first sequence number the client has not seen.
        from: u64,
    },
}

const SUBSCRIBE: u8 = 0x01;
const PUBLISH: u8 = 0x02;
const RESUME: u8 = 0x03;
const DELIVERY: u8 = 0x01;
const PAGE: u8 = 0x02;
const UNAVAILABLE: u8 = 0x03;

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
            Self::Resume { address, from } => {
                let mut out = Vec::with_capacity(10 + address.0.len());
                out.push(RESUME);
                address.encode_into(&mut out);
                out.extend_from_slice(&from.to_be_bytes());
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
                // A subscribe has no payload, so anything after the last
                // address is malformed rather than something to ignore.
                r.end()?;
                Ok(Self::Subscribe(addresses))
            }
            PUBLISH => {
                let to = r.address()?;
                Ok(Self::Publish {
                    to,
                    payload: r.rest(),
                })
            }
            RESUME => {
                let request = Self::Resume {
                    address: r.address()?,
                    from: r.u64()?,
                };
                r.end()?;
                Ok(request)
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

/// Anything the relay sends a client.
#[derive(Debug, PartialEq, Eq)]
pub enum Inbound {
    /// A frame published to an address the client follows.
    Delivery(Delivery),
    /// The end of a page of catch-up for `address`.
    Page {
        /// The address being caught up on.
        address: Address,
        /// The sequence number after the last frame sent: where to resume
        /// from if there is more.
        next: u64,
        /// True if that was everything and the client is now subscribed;
        /// false if it should send `Resume` from `next` for the rest.
        live: bool,
    },
    /// The relay cannot serve a resume for `address` from where it was asked.
    Unavailable {
        /// The address.
        address: Address,
        /// The oldest sequence number the relay still holds.
        floor: u64,
        /// The sequence number the next frame will get.
        next: u64,
    },
}

impl Inbound {
    /// Serialise for the wire.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        match self {
            Self::Delivery(delivery) => delivery.encode(),
            Self::Page {
                address,
                next,
                live,
            } => {
                let mut out = Vec::with_capacity(11 + address.0.len());
                out.push(PAGE);
                address.encode_into(&mut out);
                out.extend_from_slice(&next.to_be_bytes());
                out.push(u8::from(*live));
                out
            }
            Self::Unavailable {
                address,
                floor,
                next,
            } => {
                let mut out = Vec::with_capacity(18 + address.0.len());
                out.push(UNAVAILABLE);
                address.encode_into(&mut out);
                out.extend_from_slice(&floor.to_be_bytes());
                out.extend_from_slice(&next.to_be_bytes());
                out
            }
        }
    }

    /// Parse anything the relay sends. The input came off the network, so
    /// any shape of rubbish is an error, never a panic.
    ///
    /// # Errors
    /// If the message is empty, truncated, carries an unknown tag, an invalid
    /// address or flag, or has bytes after a message that has no payload.
    pub fn decode(wire: &[u8]) -> Result<Self> {
        let mut r = Reader(wire);
        match r.byte().map_err(|_| Error::Empty)? {
            DELIVERY => Delivery::decode(wire).map(Self::Delivery),
            PAGE => {
                let address = r.address()?;
                let next = r.u64()?;
                let live = match r.byte()? {
                    0 => false,
                    1 => true,
                    flag => return Err(Error::InvalidFlag(flag)),
                };
                r.end()?;
                Ok(Self::Page {
                    address,
                    next,
                    live,
                })
            }
            UNAVAILABLE => {
                let notice = Self::Unavailable {
                    address: r.address()?,
                    floor: r.u64()?,
                    next: r.u64()?,
                };
                r.end()?;
                Ok(notice)
            }
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

    /// Fail if anything is left: for messages with no payload, trailing
    /// bytes are malformed rather than something to ignore.
    fn end(&self) -> Result<()> {
        if self.0.is_empty() {
            Ok(())
        } else {
            Err(Error::TrailingBytes(self.0.len()))
        }
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
            Request::Resume {
                address: addr(b"channel"),
                from: u64::MAX,
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

    #[test]
    fn notices_round_trip() {
        for inbound in [
            Inbound::Page {
                address: addr(b"room"),
                next: 42,
                live: false,
            },
            Inbound::Page {
                address: addr(b"room"),
                next: 0,
                live: true,
            },
            Inbound::Unavailable {
                address: addr(&[3; 32]),
                floor: 10,
                next: 99,
            },
            Inbound::Delivery(Delivery {
                address: addr(b"room"),
                seq: 7,
                accepted_ms: 8,
                payload: b"x".to_vec(),
            }),
        ] {
            assert_eq!(Inbound::decode(&inbound.encode()).unwrap(), inbound);
        }
    }

    #[test]
    fn notices_reject_hostile_shapes() {
        let page = Inbound::Page {
            address: addr(b"a"),
            next: 1,
            live: true,
        }
        .encode();
        for cut in 0..page.len() {
            assert!(Inbound::decode(&page[..cut]).is_err(), "cut at {cut}");
        }
        let mut bad_flag = page.clone();
        *bad_flag.last_mut().unwrap() = 2;
        assert_eq!(Inbound::decode(&bad_flag), Err(Error::InvalidFlag(2)));
        let mut trailing = page;
        trailing.push(0);
        assert_eq!(Inbound::decode(&trailing), Err(Error::TrailingBytes(1)));

        let resume = Request::Resume {
            address: addr(b"a"),
            from: 3,
        }
        .encode()
        .unwrap();
        for cut in 0..resume.len() {
            assert!(Request::decode(&resume[..cut]).is_err(), "cut at {cut}");
        }
        assert_eq!(Inbound::decode(&[0x7f]), Err(Error::UnknownTag(0x7f)));
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
        // A subscribe with bytes after its last address.
        assert_eq!(
            Request::decode(&[SUBSCRIBE, 1, 1, b'a', 0xff]),
            Err(Error::TrailingBytes(1))
        );
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
