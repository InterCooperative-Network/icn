# 🏛️ AgoraNet - Civic Engine of Participatory Governance

AgoraNet is the deliberative heart of the InterCooperative Network (ICN) — a governance engine where democratic discourse, proposal deliberation, conflict resolution, and coordination all converge.

## Core Vision

AgoraNet is not just a chat app — it is a governance engine that integrates:
- Threaded deliberation for governance proposals
- Messages streams with verifiable identity via DIDs
- Deliberation to execution pipeline
- Economic and democratic flows
- Federation and identity integration

## Getting Started

### Prerequisites

- Rust toolchain (install via [rustup](https://rustup.rs/))
- PostgreSQL database server
- Docker (optional, for containerized deployment)

### Installation

1. Clone the repository:
   ```
   git clone https://github.com/yourusername/agoranet.git
   cd agoranet
   ```

2. Setup the database:
   ```
   createdb agoranet
   psql -d agoranet -f schema.sql
   ```

3. Create a `.env` file:
   ```
   DATABASE_URL=postgres://username:password@localhost/agoranet
   PORT=3000
   ```

4. Build and run:
   ```
   cargo build
   cargo run
   ```

5. Visit `http://localhost:3000/health` to confirm the API is running.

## API Endpoints

### Public Endpoints

- `GET /health` - Health check endpoint
- `GET /status` - System status information

### Thread Management

- `GET /api/threads` - List threads
- `POST /api/threads` - Create a new thread
- `GET /api/threads/:id` - Get thread details

### Message Management

- `GET /api/threads/:thread_id/messages` - List messages in a thread
- `POST /api/threads/:thread_id/messages` - Post a message to a thread

### DAG Integration

- `POST /api/dag/anchor/thread` - Anchor a thread to the DAG
- `POST /api/dag/anchor/message` - Anchor a message to the DAG

## Architecture

AgoraNet is built with Rust using the Axum web framework and PostgreSQL for data storage. Key components include:

- **Thread Model**: For deliberation on proposals, governance, etc.
- **Message Model**: With DID signatures and DAG anchoring
- **Federation**: For distributed communication between nodes
- **DAG**: For cryptographically verifiable content

## License

This project is licensed under [LICENSE DETAILS]

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.
