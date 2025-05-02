use anyhow::{Context, Result};
use prost_reflect::{DescriptorPool, MessageDescriptor};
use prost_types::FileDescriptorProto;
use std::fs;
use std::path::Path;

pub struct ProtoLoader {
    pool: DescriptorPool,
}

impl ProtoLoader {
    pub fn new() -> Self {
        ProtoLoader {
            pool: DescriptorPool::new(),
        }
    }

    /// Load a .proto file from the given path
    pub fn load_proto_file<P: AsRef<Path>>(&mut self, path: P) -> Result<()> {
        // Use protoc to compile the proto file
        let protoc = protoc_bin_vendored::protoc_bin_path()?;
        let include_path = path.as_ref().parent().unwrap_or_else(|| Path::new("."));

        // Create a temporary directory for the output
        let temp_dir = tempfile::tempdir()?;
        let output_path = temp_dir.path().join("descriptor.bin");

        // Run protoc to generate the file descriptor set
        let status = std::process::Command::new(protoc)
            .arg("--include_source_info")
            .arg("--include_imports")
            .arg(format!("--proto_path={}", include_path.display()))
            .arg(format!("--descriptor_set_out={}", output_path.display()))
            .arg(path.as_ref())
            .status()
            .with_context(|| "Failed to execute protoc")?;

        if !status.success() {
            anyhow::bail!("protoc failed with exit code: {}", status);
        }

        // Read the generated file descriptor set
        let descriptor_bytes = fs::read(&output_path)
            .with_context(|| format!("Failed to read descriptor file: {:?}", output_path))?;

        // Add the file descriptor set to the pool
        self.pool
            .decode_file_descriptor_set(&descriptor_bytes[..])?;

        Ok(())
    }

    /// Get a message descriptor by its fully qualified name
    pub fn get_message_descriptor(&self, message_name: &str) -> Result<MessageDescriptor> {
        self.pool
            .get_message_by_name(message_name)
            .with_context(|| format!("Message type not found: {}", message_name))
    }

    pub fn get_file_descriptor_proto(&self, file_name: &str) -> Result<FileDescriptorProto> {
        self.pool
            .get_file_by_name(file_name)
            .map(|f| f.file_descriptor_proto().clone())
            .with_context(|| format!("File not found: {}", file_name))
    }

    pub fn serialize_pool(&self) -> Vec<u8> {
        self.pool.encode_to_vec()
    }

    pub fn get_comment(
        &self,
        file_name: &str,
        message_name: &str,
        field_name: &str,
    ) -> Result<Option<String>> {
        match self.get_file_descriptor_proto(file_name) {
            Ok(file) => {
                if let Some(message) = file.message_type.iter().find(|m| m.name() == message_name) {
                    if let Some(field_index) =
                        message.field.iter().position(|f| f.name() == field_name)
                    {
                        if let Some(message_index) = file
                            .message_type
                            .iter()
                            .position(|m| m.name() == message_name)
                        {
                            // Build the expected path to the field:
                            // [4, message_index, 2, field_index]
                            // (we assume top-level message)
                            let path = vec![4, message_index as i32, 2, field_index as i32];

                            if let Some(source_code_info) = file.source_code_info {
                                for location in source_code_info.location.iter() {
                                    if location.path == path {
                                        return Ok(location.leading_comments.as_ref().map_or_else(
                                            || location.trailing_comments.clone(),
                                            |lead| {
                                                location.trailing_comments.as_ref().map_or(
                                                    Some(lead.clone()),
                                                    |trail| {
                                                        Some(
                                                            [lead.to_string(), trail.to_string()]
                                                                .join(" "),
                                                        )
                                                    },
                                                )
                                            },
                                        ));
                                    }
                                }
                            }
                        }
                    }
                }

                Ok(None)
            }
            Err(e) => Err(e),
        }
    }
}
