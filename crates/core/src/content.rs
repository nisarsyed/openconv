//! What goes inside an application message: MIMI content (#128).
//!
//! Pinned to [`DRAFT`]. Milestone 2 decided to adopt it rather than keep bare
//! UTF-8, as the architecture notes' "cheap alignment worth taking"
//! recommended: it is the most interoperability-relevant thing inside the
//! encryption, and it gives every message a stable id, which replies, edits,
//! reactions and a deduplicated transcript all hang off.
//!
//! A message is a deterministic-CBOR array (§4.1, §6.1):
//!
//! ```text
//! [ salt, replaces, topicId, expires, inReplyTo, extensions, nestedPart ]
//! ```
//!
//! and its id is `0x01 || SHA-256(senderUriLen || senderUri || roomUriLen ||
//! roomUri || content || salt)[0..31]` (§3.3), checked against the draft's own
//! worked example in the tests below.
//!
//! # URIs before identity
//!
//! Message ids bind a sender and a room URI, and real URIs are milestone 3.
//! Until then they are provisional: the room's from the MLS group id, the
//! sender's from a hash of the signature key MLS authenticated the message
//! with. Neither is taken from the content, so a member cannot claim another's
//! URI. Neither is written into the content's extensions either: a receiver
//! derives both from the MLS context, so carrying them would only add a claim
//! to check.

use sha2::{Digest, Sha256};

/// The version of `draft-ietf-mimi-content` this implements. The draft may
/// still move; anything that depends on its exact bytes names this.
pub const DRAFT: &str = "draft-ietf-mimi-content-09";

/// The largest content accepted, in bytes. The relay refuses messages over
/// 1 MiB (#122), so nothing larger can arrive legitimately.
const MAX_CONTENT: usize = 1024 * 1024;

/// How deeply CBOR may nest. The draft allows parts four levels deep (§6.3);
/// the array, its part and that part's fields are the rest.
const MAX_DEPTH: usize = 16;

/// A message id: the hash algorithm's identifier, then 31 bytes of hash.
pub type MessageId = [u8; 32];

/// Anything wrong with a message's content.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    /// Not well-formed CBOR, nested too deeply, or too large.
    #[error("content is not valid CBOR")]
    Cbor,
    /// Well-formed CBOR, but not the shape §4.1 defines.
    #[error("content does not match {DRAFT}: {0}")]
    Shape(&'static str),
}

/// A MIMI content document, as far as this client uses one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Content {
    /// Random per message; part of what makes its id unique.
    pub salt: [u8; 16],
    /// For an edit or a delete: the id of the first version of the message.
    pub replaces: Option<MessageId>,
    /// Opaque; empty when the room has no topics.
    pub topic_id: Vec<u8>,
    /// For a reply or a reaction: the id of the message it answers.
    pub in_reply_to: Option<MessageId>,
    /// What the message says.
    pub body: Body,
}

/// A message's body: the nested part, reduced to what this client shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Body {
    /// No content: a delete, or an unlike (§5.6, §5.7).
    Null,
    /// One part with its media type, such as `text/plain;charset=utf-8`.
    Single {
        /// How to show it: 1 renders it, 2 is a reaction (§4.4).
        disposition: u8,
        /// Its media type.
        content_type: String,
        /// Its bytes.
        content: Vec<u8>,
    },
    /// An external or multipart body: valid, but not shown yet.
    Unsupported,
}

/// The media type of a plain message.
pub const TEXT: &str = "text/plain;charset=utf-8";

/// The `render` disposition: show the part inline (§4.4).
const RENDER: u8 = 1;

impl Content {
    /// A plain-text message with a fresh random salt.
    #[must_use]
    pub fn text(text: &str, salt: [u8; 16]) -> Self {
        Self {
            salt,
            replaces: None,
            topic_id: Vec::new(),
            in_reply_to: None,
            body: Body::Single {
                disposition: RENDER,
                content_type: TEXT.to_owned(),
                content: text.as_bytes().to_vec(),
            },
        }
    }

    /// The text to show for this message, if it has any.
    #[must_use]
    pub fn as_text(&self) -> Option<&str> {
        match &self.body {
            Body::Single {
                content_type,
                content,
                ..
            } if content_type.starts_with("text/") => std::str::from_utf8(content).ok(),
            _ => None,
        }
    }

    /// Deterministic CBOR (§6.1): definite lengths, smallest encodings, and
    /// no maps but the empty extensions map, so no key order to choose.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        use ciborium::value::Value;
        let id = |id: &Option<MessageId>| id.map_or(Value::Null, |id| Value::Bytes(id.to_vec()));
        let part = match &self.body {
            Body::Single {
                disposition,
                content_type,
                content,
            } => vec![
                Value::Integer((*disposition).into()),
                Value::Text(String::new()),
                Value::Integer(1.into()),
                Value::Text(content_type.clone()),
                Value::Bytes(content.clone()),
            ],
            // An unsupported body is never built by this client; decoding
            // keeps it only as a marker, so it encodes as empty.
            Body::Null | Body::Unsupported => vec![
                Value::Integer(0.into()),
                Value::Text(String::new()),
                Value::Integer(0.into()),
            ],
        };
        let document = Value::Array(vec![
            Value::Bytes(self.salt.to_vec()),
            id(&self.replaces),
            Value::Bytes(self.topic_id.clone()),
            Value::Null, // expires: never
            id(&self.in_reply_to),
            Value::Map(Vec::new()),
            Value::Array(part),
        ]);
        let mut out = Vec::new();
        // Writing to a Vec cannot fail.
        let _ = ciborium::ser::into_writer(&document, &mut out);
        out
    }

    /// Parse content off the wire. It is hostile until this passes: too
    /// large, too deep, or the wrong shape is an error, never a panic.
    ///
    /// # Errors
    /// If `bytes` is not a §4.1 document.
    pub fn decode(bytes: &[u8]) -> Result<Self, Error> {
        use ciborium::value::Value;
        if bytes.len() > MAX_CONTENT {
            return Err(Error::Cbor);
        }
        let value: Value = ciborium::de::from_reader_with_recursion_limit(bytes, MAX_DEPTH)
            .map_err(|_| Error::Cbor)?;
        let Value::Array(fields) = value else {
            return Err(Error::Shape("not an array"));
        };
        let [
            salt,
            replaces,
            topic,
            expires,
            in_reply_to,
            extensions,
            part,
        ]: [Value; 7] = fields
            .try_into()
            .map_err(|_| Error::Shape("not seven fields"))?;
        let salt = bytes_of(salt)?
            .try_into()
            .map_err(|_| Error::Shape("salt is not 16 bytes"))?;
        let id = |v: Value| -> Result<Option<MessageId>, Error> {
            match v {
                Value::Null => Ok(None),
                other => bytes_of(other)?
                    .try_into()
                    .map(Some)
                    .map_err(|_| Error::Shape("a message id is not 32 bytes")),
            }
        };
        let replaces = id(replaces)?;
        let in_reply_to = id(in_reply_to)?;
        let topic_id = bytes_of(topic)?;
        if !matches!(expires, Value::Null | Value::Array(_)) {
            return Err(Error::Shape("expires"));
        }
        // Extensions are allowed and ignored: none this client acts on.
        if !matches!(extensions, Value::Map(_)) {
            return Err(Error::Shape("extensions is not a map"));
        }
        Ok(Self {
            salt,
            replaces,
            topic_id,
            in_reply_to,
            body: body_of(part)?,
        })
    }
}

fn bytes_of(v: ciborium::value::Value) -> Result<Vec<u8>, Error> {
    match v {
        ciborium::value::Value::Bytes(b) => Ok(b),
        _ => Err(Error::Shape("expected bytes")),
    }
}

fn body_of(part: ciborium::value::Value) -> Result<Body, Error> {
    use ciborium::value::Value;
    let Value::Array(fields) = part else {
        return Err(Error::Shape("nestedPart is not an array"));
    };
    let small = |v: &Value| -> Result<u8, Error> {
        match v {
            Value::Integer(i) => u8::try_from(*i).map_err(|_| Error::Shape("out of range")),
            _ => Err(Error::Shape("expected an integer")),
        }
    };
    let (Some(disposition), Some(Value::Text(_)), Some(cardinality)) =
        (fields.first(), fields.get(1), fields.get(2))
    else {
        return Err(Error::Shape("nestedPart"));
    };
    let disposition = small(disposition)?;
    match (small(cardinality)?, fields.len()) {
        (0, 3) => Ok(Body::Null),
        (1, 5) => match (&fields[3], &fields[4]) {
            (Value::Text(content_type), Value::Bytes(content)) => Ok(Body::Single {
                disposition,
                content_type: content_type.clone(),
                content: content.clone(),
            }),
            _ => Err(Error::Shape("single part")),
        },
        (2 | 3, _) => Ok(Body::Unsupported),
        _ => Err(Error::Shape("cardinality")),
    }
}

/// A message's id (§3.3).
#[must_use]
pub fn message_id(sender_uri: &str, room_uri: &str, content: &[u8], salt: &[u8; 16]) -> MessageId {
    let mut hash = Sha256::new();
    for uri in [sender_uri, room_uri] {
        // URIs this long cannot be built here; saturating keeps it total.
        hash.update(u16::try_from(uri.len()).unwrap_or(u16::MAX).to_be_bytes());
        hash.update(uri.as_bytes());
    }
    hash.update(content);
    hash.update(salt);
    let digest = hash.finalize();
    let mut id = [0u8; 32];
    id[0] = 0x01; // SHA-256, from the named-information registry
    id[1..].copy_from_slice(&digest[..31]);
    id
}

/// The provisional URI of a room, until identity (milestone 3).
#[must_use]
pub fn room_uri(group_id: &[u8]) -> String {
    format!("openconv-provisional:room/{}", hex(group_id))
}

/// The provisional URI of a sender, from the signature key MLS authenticated
/// them with, until identity (milestone 3).
#[must_use]
pub fn sender_uri(signature_key: &[u8]) -> String {
    format!(
        "openconv-provisional:user/{}",
        hex(&Sha256::digest(signature_key)[..16])
    )
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The ORIGINAL example of §5.1, bytes as printed in the draft.
    const DRAFT_EXAMPLE: &str = "87505eed9406c2545547ab6f09f20a18b003f640f6f6a201782\
        06d696d693a2f2f6578616d706c652e636f6d2f752f616c6963652d736d69746802782\
        56d696d693a2f2f6578616d706c652e636f6d2f722f656e67696e656572696e675f746\
        5616d85016001781e746578742f6d61726b646f776e3b76617269616e743d47464d2d4\
        d494d49583948692065766572796f6e652c207765206a7573742073686970706564207\
        2656c6561736520322e302e205f5f476f6f642020776f726b5f5f21";
    const DRAFT_ID: &str = "017ce54837404c3696e0c747b985cb172716d0ed0a3d249ca63ace7d82a096f4";

    fn unhex(s: &str) -> Vec<u8> {
        let s: String = s.chars().filter(char::is_ascii_hexdigit).collect();
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }

    /// Our id derivation reproduces the draft's own worked example exactly,
    /// so it is §3.3 and not a reading of it.
    #[test]
    fn message_ids_match_the_drafts_example() {
        let bytes = unhex(DRAFT_EXAMPLE);
        let salt: [u8; 16] = bytes[2..18].try_into().unwrap();
        let id = message_id(
            "mimi://example.com/u/alice-smith",
            "mimi://example.com/r/engineering_team",
            &bytes,
            &salt,
        );
        assert_eq!(id.to_vec(), unhex(DRAFT_ID));
    }

    /// The draft's example decodes, as the right fields.
    #[test]
    fn the_drafts_example_decodes() {
        let content = Content::decode(&unhex(DRAFT_EXAMPLE)).unwrap();
        assert_eq!(
            content.salt.to_vec(),
            unhex("5eed9406c2545547ab6f09f20a18b003")
        );
        assert_eq!(content.replaces, None);
        assert!(content.topic_id.is_empty());
        assert_eq!(
            content.as_text(),
            Some("Hi everyone, we just shipped release 2.0. __Good  work__!")
        );
    }

    /// Encoding is deterministic CBOR: a document with the draft example's
    /// fields (minus the URI extensions, which we derive rather than carry)
    /// encodes to exactly the draft's bytes for those fields.
    #[test]
    fn encoding_matches_the_drafts_bytes_field_for_field() {
        let content = Content {
            salt: unhex("5eed9406c2545547ab6f09f20a18b003")
                .try_into()
                .unwrap(),
            replaces: None,
            topic_id: Vec::new(),
            in_reply_to: None,
            body: Body::Single {
                disposition: 1,
                content_type: "text/markdown;variant=GFM-MIMI".into(),
                content: b"Hi everyone, we just shipped release 2.0. __Good  work__!".to_vec(),
            },
        };
        let ours = content.encode();
        let draft = unhex(DRAFT_EXAMPLE);
        // The draft's document with its extensions map (a2 ... 5f7465616d)
        // replaced by an empty one (a0).
        let ext_start = 1 + 17 + 1 + 1 + 1 + 1;
        let ext_end = ext_start + 1 + 1 + 2 + 32 + 1 + 2 + 37;
        let mut expected = draft[..ext_start].to_vec();
        expected.push(0xa0);
        expected.extend_from_slice(&draft[ext_end..]);
        assert_eq!(ours, expected);
        assert_eq!(
            Content::decode(&ours).unwrap(),
            content,
            "does not round-trip"
        );
    }

    #[test]
    fn text_round_trips_and_ids_differ_by_salt() {
        let a = Content::text("hello", [1; 16]);
        let b = Content::text("hello", [2; 16]);
        assert_eq!(Content::decode(&a.encode()).unwrap(), a);
        assert_eq!(a.as_text(), Some("hello"));
        let room = room_uri(b"group");
        let me = sender_uri(b"key");
        assert_ne!(
            message_id(&me, &room, &a.encode(), &a.salt),
            message_id(&me, &room, &b.encode(), &b.salt)
        );
        assert_ne!(
            message_id(&me, &room, &a.encode(), &a.salt),
            message_id(&sender_uri(b"other key"), &room, &a.encode(), &a.salt),
            "the id does not bind the sender"
        );
    }

    /// Content comes off the network: every truncation, a wrong shape, a
    /// deep nest and an oversized document are errors, never panics.
    #[test]
    fn hostile_content_is_an_error() {
        let good = Content::text("hello", [3; 16]).encode();
        for cut in 0..good.len() {
            assert!(Content::decode(&good[..cut]).is_err(), "cut at {cut}");
        }
        assert!(Content::decode(b"plain old utf-8").is_err());
        let mut deep = vec![0x81; 200];
        deep.push(0x00);
        assert_eq!(Content::decode(&deep), Err(Error::Cbor));
        assert_eq!(
            Content::decode(&vec![0x40; MAX_CONTENT + 1]),
            Err(Error::Cbor)
        );
        // Seven fields, but a 15-byte salt.
        let mut short_salt = good.clone();
        short_salt[1] = 0x4f;
        short_salt.remove(2);
        assert!(Content::decode(&short_salt).is_err());
    }
}
