use std::io::Result;

fn main() -> Result<()> {
    let protos = &["src/calls.proto", "src/responses.proto"];
    prost_build::compile_protos(protos, &["src/"])
}
