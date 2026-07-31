//! Must NOT compile: the CS-3 3.2 digest types are sealed (private field). Only the transport
//! decoder mints them via `from_transport_digest`; no external literal can fabricate one — so the
//! engine can never inject a digest it did not receive from the transport.

use prro_domain::delivery::{DecodedResponseDigest, GrpcStatusDigest};

fn main() {
    // Private field → "cannot initialize a tuple struct which contains private fields" (E0423).
    let _decoded = DecodedResponseDigest([0u8; 32]);
    let _grpc = GrpcStatusDigest([0u8; 32]);
}
