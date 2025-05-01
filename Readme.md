# Proto Faker

A command-line tool for generating random protobuf messages based on .proto schema definitions. Proto Faker can generate realistic test data and publish it to Kafka topics with schema registry integration.

## Features

- Generate random protobuf messages from .proto schema files
- Print messages to stdout or publish to Kafka topics
- Schema registry integration for Kafka publishing
- Customizable field generation via proto comments
- Value pools for consistent data across messages
- Support for various distribution patterns (uniform, normal, log-normal, Pareto)

## Usage

```
proto-faker [OPTIONS] <COMMAND>
```

### Commands

- `print`: Generate and print messages to stdout
- `publish`: Generate and publish messages to a Kafka topic

### Common Options

```
-f, --proto-file <PROTO_FILE>    Path to the .proto file
-m, --message-type <MESSAGE_TYPE>    Message type to generate (fully qualified name)
-c, --count <COUNT>    Number of messages to generate [default: 1]
-p, --pools <POOLS>    Define value pools for consistent data generation
-k, --key <KEY>    Kafka key field (default: 'id')
```

### Publish Options

```
-b, --broker <BROKER>    Kafka broker address
-t, --topic <TOPIC>    Kafka topic to publish to
-s, --schema-registry <SCHEMA_REGISTRY>    Schema registry URL
```

### Pool Configuration

Pools allow you to create sets of consistent values that can be reused across messages:

```
-p name:count:type
```

Where:
- `name`: Pool identifier
- `count`: Number of items in the pool
- `type`: Data type (i32, i64, u32, u64, f32, f64, string, bytes, uuid)

Example:
```
-p user_ids:100:uuid -p product_names:50:string
```

## Field Generation Options

Field generation can be customized using comments in the .proto file:

```protobuf
message Person {
  // words=1..3
  string name = 1;
  
  // string=uuid
  string id = 2;
  
  // count=1..5 
  repeated string tags = 3;
  
  // pool=user_ids
  string user_id = 4;
  
  // distribution=normal(0,1)
  double score = 5;
}
```

### Available Options

- `words=N` or `words=N..M`: Generate string with N or N-M words
- `count=N` or `count=N..M`: Generate N or N-M items for repeated fields
- `string=uuid`: Generate a UUID string
- `pool=name`: Use values from the specified pool
- `distribution=type(params)`: Use specific distribution for numeric values:
  - `distribution=uniform`: Uniform distribution
  - `distribution=normal(mean,stddev)`: Normal distribution
  - `distribution=log_normal(mean,stddev)`: Log-normal distribution
  - `distribution=pareto(scale,shape)`: Pareto distribution

## Examples

Print a single Person message:
```
proto-faker print -f proto/person.proto -m person.Person
```

Generate 10 messages and publish to Kafka:
```
proto-faker publish -f proto/person.proto -m person.Person -c 10 -b localhost:9092 -t person-topic -s http://localhost:8081
```

Generate messages with consistent user IDs:
```
proto-faker print -f proto/person.proto -m person.Person -c 5 -p user_ids:20:uuid
```
