mod distribution;
mod option_parser;
mod proto_faker;
mod proto_loader;

use anyhow::{Context, Result, anyhow};
use clap::{Parser, Subcommand};
use indicatif::{ProgressBar, ProgressStyle};
use prost_reflect::prost::Message;
use prost_reflect::{DynamicMessage, MessageDescriptor, ReflectMessage, Value};
use proto_faker::ProtoFaker;
use proto_loader::ProtoLoader;
use rayon::iter::ParallelIterator;
use rdkafka::config::ClientConfig;
use rdkafka::producer::{FutureProducer, FutureRecord};
use schema_registry_converter::async_impl::proto_raw::ProtoRawEncoder;
use schema_registry_converter::async_impl::schema_registry::{self, SrSettings};
use schema_registry_converter::schema_registry_common::{SubjectNameStrategy, SuppliedSchema};
use std::fs;
use std::io::{Seek, Write};
use std::path::PathBuf;
use std::time::Duration;
use zstd::stream::Encoder;

#[derive(Parser, Debug)]
#[command(author, version, about = "Generate random protobuf messages")]
struct Args {
    #[command(subcommand)]
    cmd: Commands,
}

#[derive(clap::Args, Debug, PartialEq, Clone)]
struct Common {
    /// Path to the .proto file
    #[arg(short = 'f', long)]
    proto_file: PathBuf,

    /// Message type to generate (fully qualified name)
    #[arg(short, long)]
    message_type: String,

    /// Number of messages to generate
    #[arg(short, long, default_value_t = 1)]
    count: usize,

    #[arg(short, long, value_parser = option_parser::parse_pool_config)]
    pools: Option<Vec<PoolConfig>>,

    /// Kafka key field (default: 'id')
    #[arg(short, long)]
    key: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
struct PoolConfig {
    name: String,
    items: usize,
    value: option_parser::ValueType,
}

#[derive(Subcommand, Debug, PartialEq)]
enum Commands {
    Print {
        #[command(flatten)]
        common: Common,
    },
    Publish {
        #[command(flatten)]
        common: Common,

        /// Kafka broker address (required if publish is set)
        #[arg(short, long)]
        broker: String,

        /// Kafka topic to publish to (required if publish is set)
        #[arg(short, long)]
        topic: String,

        /// Schema registry URL (required if publish is set)
        #[arg(short, long)]
        schema_registry: String,
    },
    Write {
        #[command(flatten)]
        common: Common,

        /// Output zip file path
        #[arg(short, long)]
        output: PathBuf,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let common = match &args.cmd {
        Commands::Print { common } => common,
        Commands::Publish { common, .. } => common,
        Commands::Write { common, .. } => common,
    };

    println!("Loading proto file: {}", common.proto_file.display());
    println!("Using message type: {}", common.message_type);
    println!("Generating {} message(s)", common.count);

    // Load the proto file
    let mut loader = ProtoLoader::new();
    loader.load_proto_file(&common.proto_file).unwrap();

    // Get the message descriptor
    let message_descriptor = loader.get_message_descriptor(&common.message_type).unwrap();
    println!("Found message type: {}", message_descriptor.full_name());

    let (tx, messages) = std::sync::mpsc::sync_channel(100);
    let loader1 = loader.clone();
    let pools = common.pools.clone().unwrap_or(vec![]);
    let message_descriptor1 = message_descriptor.clone();
    let count = common.count;
    std::thread::spawn(move || {
        let faker = ProtoFaker::new(pools);

        rayon::iter::repeatn((), count).for_each(|_| {
            let msg = faker
                .generate_dynamic(&loader1, &message_descriptor1)
                .unwrap();

            tx.send(msg).unwrap();
        });
    });
    let bar = ProgressBar::new(common.count as u64);
    bar.set_style(
        ProgressStyle::with_template(
            "[{elapsed_precise} / {eta_precise}] {bar:40.cyan/blue} {pos:>7}/{len:7} {msg} [{per_sec}]",
        )
        .unwrap()
        .progress_chars("##-"),
    );
    let messages = messages.into_iter().inspect(|_| bar.inc(1));

    match args.cmd {
        Commands::Publish {
            broker,
            topic,
            schema_registry,
            common,
        } => {
            let producer = ClientConfig::new()
                .set("bootstrap.servers", broker)
                .set("message.timeout.ms", "5000")
                .create::<FutureProducer>()
                .context("Failed to create Kafka producer")?;

            let sr_settings = SrSettings::new(schema_registry);

            let schema = SuppliedSchema {
                name: Some(message_descriptor.full_name().to_string()),
                schema_type:
                    schema_registry_converter::schema_registry_common::SchemaType::Protobuf,
                schema: fs::read_to_string(common.proto_file.clone())
                    .context("Can't read proto file")?,
                references: vec![],
            };

            schema_registry::post_schema(
                &sr_settings,
                message_descriptor.full_name().to_string(),
                schema,
            )
            .await
            .context("Failed to publish schema")?;

            let encoder = ProtoRawEncoder::new(sr_settings);

            // Determine the key field
            let key_field = common.key.as_deref().unwrap_or("id");

            let fs = messages.into_iter().map(|message| {
                publish_to_kafka(
                    &producer,
                    &encoder,
                    &topic,
                    message_descriptor.full_name(),
                    message,
                    &message_descriptor,
                    key_field,
                )
            });

            futures::future::try_join_all(fs).await?;
        }
        Commands::Print { common } => {
            for (i, message) in messages.enumerate() {
                if common.count > 1 {
                    println!("\n--- Message {} ---", i + 1);
                }

                for field in message_descriptor.fields() {
                    let value = message.get_field(&field);
                    print_field_value(&message_descriptor, field.name(), &value, 2);
                }
            }
        }
        Commands::Write { common, output } => {
            println!("Writing messages to zst file: {}", output.display());
            // Determine the key field
            let key_field = common.key.as_deref().unwrap_or("id");

            write_to_zstd_file(&loader, key_field, messages, &message_descriptor, &output)?;
            println!(
                "Successfully wrote {} messages to {}",
                common.count,
                output.display()
            );
        }
    }

    Ok(())
}

async fn publish_to_kafka(
    producer: &FutureProducer,
    encoder: &ProtoRawEncoder<'_>,
    topic: &str,
    schema_name: &str,
    message: DynamicMessage,
    message_descriptor: &MessageDescriptor,
    key_field: &str,
) -> Result<()> {
    // Find the field descriptor for the key field
    let field_desc = message_descriptor
        .fields()
        .find(|f| f.name() == key_field)
        .ok_or_else(|| anyhow!("Key field '{}' not found in message", key_field))?;

    // Extract the key value from the message
    let key_value = message.get_field(&field_desc);

    let key = match key_value.as_ref() {
        Value::String(s) => s.clone(),
        Value::I32(i) => i.to_string(),
        Value::I64(i) => i.to_string(),
        Value::Bool(b) => b.to_string(),
        _ => return Err(anyhow!("Unsupported key type: {:?}", key_value)),
    };

    // Encode the message with the schema registry
    let encoded_payload = encoder
        .encode(
            &message.encode_to_vec(),
            schema_name,
            SubjectNameStrategy::RecordNameStrategy(schema_name.to_string()),
        )
        .await
        .context("Failed to encode message with schema registry")?;

    // Create a record to send
    let record = FutureRecord::to(topic).payload(&encoded_payload).key(&key);

    // Send the record
    let delivery_status = producer
        .send(record, Duration::from_secs(5))
        .await
        .map_err(|(kafka_error, _message)| kafka_error)
        .context("Failed to send message to Kafka")?;

    println!(
        "Message published to topic {} at partition {} with offset {}",
        topic, delivery_status.0, delivery_status.1
    );

    Ok(())
}

fn write_to_zstd_file(
    loader: &ProtoLoader,
    key_field: &str,
    messages: impl Iterator<Item = DynamicMessage>,
    message_descriptor: &MessageDescriptor,
    output_path: &PathBuf,
) -> Result<()> {
    // Create a new file for the compressed output
    let mut file = fs::File::create(output_path).context("Failed to create output file")?;

    // Write empty u32 for number of messages
    file.write_all(&[0u8; 4])?;

    // Create a zstd encoder with default compression level
    let mut encoder = Encoder::new(file, 3)?;

    let file_descriptor_set = loader.serialize_pool();
    let file_descriptor_set_len = file_descriptor_set.len() as u32;

    encoder.write_all(&file_descriptor_set_len.to_le_bytes())?;
    encoder.write_all(&file_descriptor_set)?;

    // Find the field descriptor for the key field
    let field_desc = message_descriptor
        .fields()
        .find(|f| f.name() == key_field)
        .ok_or_else(|| anyhow!("Key field '{}' not found in message", key_field))?;

    // Encode each message with length delimiter and write to the buffer
    let mut c: u32 = 0;
    for message in messages {
        // Extract the key value from the message
        let key_value = message.get_field(&field_desc);

        let key = match key_value.as_ref() {
            Value::String(s) => s.clone(),
            Value::I32(i) => i.to_string(),
            Value::I64(i) => i.to_string(),
            Value::Bool(b) => b.to_string(),
            _ => return Err(anyhow!("Unsupported key type: {:?}", key_value)),
        };

        let key_bytes = key.as_bytes();
        let key_len = key_bytes.len() as u32;
        let key_len_bytes = key_len.to_le_bytes();

        let bytes = message.encode_to_vec();
        let len = bytes.len() as u32;
        let len_bytes = len.to_le_bytes();

        encoder.write_all(&key_len_bytes)?;
        encoder.write_all(key_bytes)?;

        encoder.write_all(&len_bytes)?;
        encoder.write_all(&bytes)?;

        c += 1;
    }
    println!("Written: {} messages", c);

    encoder.flush()?;
    let mut writer = encoder.finish()?;
    // Write num messages to front of file
    writer.flush()?;
    writer.seek(std::io::SeekFrom::Start(0))?;
    writer.write_all(&c.to_le_bytes())?;
    writer.flush()?;

    Ok(())
}

fn print_field_value(
    message_descriptor: &MessageDescriptor,
    name: &str,
    value: &Value,
    indent: usize,
) {
    let indent_str = " ".repeat(indent);

    match value {
        Value::Message(msg) => {
            println!("{}{}:", indent_str, name);
            for field in msg.descriptor().fields() {
                let field_value = msg.get_field(&field);
                print_field_value(&msg.descriptor(), field.name(), &field_value, indent + 2);
            }
        }
        Value::List(values) => {
            println!("{}{}:", indent_str, name);
            for (i, val) in values.iter().enumerate() {
                print_field_value(message_descriptor, &format!("[{}]", i), val, indent + 2);
            }
        }
        Value::EnumNumber(value) => {
            if let Some(field) = message_descriptor.fields().find(|f| f.name() == name) {
                // Check if this field is an enum type
                if let prost_reflect::Kind::Enum(enum_descriptor) = field.kind() {
                    if let Some(enum_value) =
                        enum_descriptor.values().find(|v| v.number() == *value)
                    {
                        println!("{}{}: {} ({})", indent_str, name, enum_value.name(), value);
                        return;
                    }
                }
            }
            // Fallback if we can't find the enum descriptor or value
            println!("{}{}: ENUM_VALUE ({})", indent_str, name, value);
        }
        _ => println!("{}{}: {:?}", indent_str, name, value),
    }
}
