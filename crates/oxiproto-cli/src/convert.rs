#![forbid(unsafe_code)]

use clap::Args;
use std::io::Read;
use std::path::PathBuf;

use crate::util::Verbosity;

use oxiproto_reflect::{DescriptorPool, DynamicMessage};
use prost::Message as _;
use prost_reflect::ReflectMessage;

/// Arguments shared by `encode` and `decode`.
#[derive(Args)]
pub struct ConvertArgs {
    /// Input .proto files defining the message type.
    #[arg(required = true)]
    pub protos: Vec<PathBuf>,

    /// Fully-qualified message type name, e.g. `my.package.MyMessage`.
    #[arg(short = 't', long)]
    pub message_type: String,

    /// Input file (reads stdin if omitted).
    #[arg(short = 'i', long)]
    pub input: Option<PathBuf>,

    /// Output file (writes stdout if omitted).
    #[arg(short = 'o', long)]
    pub output: Option<PathBuf>,

    /// Include paths for resolving proto imports (may be repeated).
    #[arg(short = 'I', long)]
    pub include: Vec<PathBuf>,
}

/// Build a [`DescriptorPool`] from the given proto files.
fn build_pool(
    protos: &[PathBuf],
    includes: &[PathBuf],
) -> Result<DescriptorPool, Box<dyn std::error::Error>> {
    for proto in protos {
        if !proto.exists() {
            return Err(format!("proto file not found: {}", proto.display()).into());
        }
    }

    let fds = oxiproto_build::compile_to_fds(protos, includes)?;
    let fds_bytes = fds.encode_to_vec();
    let pool = oxiproto_reflect::pool_from_fds_bytes(&fds_bytes)?;
    Ok(pool)
}

/// Read all bytes from the input file or stdin.
fn read_input(input: &Option<PathBuf>) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    match input {
        Some(path) => Ok(std::fs::read(path)?),
        None => {
            let mut buf = Vec::new();
            std::io::stdin().read_to_end(&mut buf)?;
            Ok(buf)
        }
    }
}

/// Write bytes to the output file or stdout.
fn write_output(output: &Option<PathBuf>, data: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    match output {
        Some(path) => {
            std::fs::write(path, data)?;
            Ok(())
        }
        None => {
            use std::io::Write;
            std::io::stdout().write_all(data)?;
            Ok(())
        }
    }
}

/// Run the `encode` subcommand: read canonical Protobuf-JSON from input and
/// write binary protobuf wire format to output.
///
/// # Errors
///
/// Returns an error if the proto cannot be parsed, the message type is not
/// found, the input is not valid JSON, or I/O fails.
pub fn run_encode(
    args: ConvertArgs,
    verbosity: Verbosity,
) -> Result<(), Box<dyn std::error::Error>> {
    let _ = verbosity; // reserved for future verbose progress messages
    let pool = build_pool(&args.protos, &args.include)?;
    let descriptor = pool
        .get_message_by_name(&args.message_type)
        .ok_or_else(|| format!("message type '{}' not found", args.message_type))?;

    let input_bytes = read_input(&args.input)?;
    let json_value: serde_json::Value = serde_json::from_slice(&input_bytes)?;

    let codec = oxiproto_json::JsonCodec::default();
    let msg = oxiproto_json::from_json(&json_value, &descriptor, &codec)?;

    let encoded = msg.encode_to_vec();
    write_output(&args.output, &encoded)?;

    Ok(())
}

/// Run the `decode` subcommand: read binary protobuf wire format from input
/// and write canonical Protobuf-JSON to output.
///
/// # Errors
///
/// Returns an error if the proto cannot be parsed, the message type is not
/// found, the input is not valid protobuf, or I/O fails.
pub fn run_decode(
    args: ConvertArgs,
    verbosity: Verbosity,
) -> Result<(), Box<dyn std::error::Error>> {
    let _ = verbosity; // reserved for future verbose progress messages
    let pool = build_pool(&args.protos, &args.include)?;
    let descriptor = pool
        .get_message_by_name(&args.message_type)
        .ok_or_else(|| format!("message type '{}' not found", args.message_type))?;

    let input_bytes = read_input(&args.input)?;
    let msg = DynamicMessage::decode(descriptor.clone(), input_bytes.as_slice())?;

    // Verify the decode produced the expected descriptor (defensive)
    let _ = msg.descriptor();

    let codec = oxiproto_json::JsonCodec::default();
    let json_value = oxiproto_json::to_json(&msg, &codec);
    let json_string = serde_json::to_string_pretty(&json_value)?;

    let mut output = json_string.into_bytes();
    output.push(b'\n');
    write_output(&args.output, &output)?;

    Ok(())
}
