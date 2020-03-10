use tokio::io::{Stdin, Stdout};
use tokio_util::codec::length_delimited::{Builder, LengthDelimitedCodec};
use tokio_util::codec::{FramedRead, FramedWrite};

pub fn writer(stdout: Stdout) -> FramedWrite<Stdout, LengthDelimitedCodec> {
    Builder::new().native_endian().new_write(stdout)
}

pub fn reader(stdin: Stdin) -> FramedRead<Stdin, LengthDelimitedCodec> {
    Builder::new().native_endian().new_read(stdin)
}
