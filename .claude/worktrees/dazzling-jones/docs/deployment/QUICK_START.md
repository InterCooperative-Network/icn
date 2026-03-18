# 🚀 ICN Quick Start - Full Stack

**Status:** ✅ PRODUCTION READY  
**Date:** 2025-12-12

## 🎯 What We Have

```
┌─────────────────────────────────────────────────────────┐
│                    ICN Full Stack                        │
├─────────────────────────────────────────────────────────┤
│  Mobile Wallet (CoopWallet)    ←→  Gateway API          │
│  • 11 screens implemented          • REST + WebSocket   │
│  • Real-time updates               • Auth + payments    │
│  • Offline mode                    • Governance + trust │
│  • QR payments                     • Identity (SDIS)    │
│  • Governance voting               • Metrics/monitoring │
│  • Identity verification                                │
│  • Trust attestations              Backend Services     │
│                                    • ICN daemon (icnd)  │
│  Web UI (Pilot)                    • Gossip protocol    │
│  • Member directory                • Ledger sync        │
│  • Payment flows                   • Trust graph        │
│  • Proposal voting                 • Governance         │
│  • Dashboard                       • Distributed compute│
└─────────────────────────────────────────────────────────┘
```

## 🏃 One-Command Deployment

```bash
# Start everything (daemon, gateway, pilot-ui, metrics)
docker-compose -f docker-compose.full.yml up -d

# Check status
docker-compose -f docker-compose.full.yml ps

# View logs
docker-compose -f docker-compose.full.yml logs -f gateway

# Stop everything
docker-compose -f docker-compose.full.yml down
```

## 📱 Mobile Wallet

### Development

```bash
cd sdk/react-native/examples/CoopWallet
npm install
npm start   # Opens Metro bundler
```

Then:
- Press `i` for iOS simulator
- Press `a` for Android emulator
- Scan QR with Expo Go app

### Production Build

```bash
# Install Expo CLI globally
npm install -g eas-cli

# iOS
eas build --platform ios --profile production

# Android
eas build --platform android --profile production
```

## 🌐 Web UI

### Development

```bash
cd web/pilot-ui
npm install
npm run dev   # http://localhost:5173
```

### Production

```bash
# Build
npm run build

# Or use Docker
docker build -t icn-pilot-ui .
docker run -p 8080:80 icn-pilot-ui
```

## 🔧 Backend Services

### Local Development

```bash
cd icn

# Build everything
cargo build

# Run daemon
./target/debug/icnd

# Run gateway (separate terminal)
cd icn/crates/icn-gateway
cargo run
```

### Testing

```bash
# Run all tests
cargo test

# Run specific crate tests
cargo test -p icn-gateway

# Run with logging
RUST_LOG=debug cargo test
```

## 📊 Access Points

### Development

| Service | URL | Description |
|---------|-----|-------------|
| Gateway API | http://localhost:3030 | REST + WebSocket |
| Pilot UI | http://localhost:5173 | Web interface |
| Mobile App | Expo Metro | React Native app |
| Metrics | http://localhost:9090 | Prometheus |
| Dashboard | http://localhost:3000 | Grafana |

### Production

| Service | URL | Description |
|---------|-----|-------------|
| Gateway API | https://api.icn.zone | REST + WebSocket |
| Pilot UI | https://pilot.icn.zone | Web interface |
| Mobile App | App Store / Play Store | iOS/Android |
| Metrics | https://metrics.icn.zone | Monitoring |

## 🔑 Key Features

### Mobile Wallet
- ✅ **Login** - Cooperative ID + secure keys
- ✅ **Payments** - Send/receive with QR codes
- ✅ **Governance** - View proposals, cast votes
- ✅ **Identity** - SDIS Level 1 verification
- ✅ **Trust** - Attest trust for members
- ✅ **Offline** - Queue operations, auto-retry

### Web UI
- ✅ **Dashboard** - Overview of cooperative
- ✅ **Members** - Directory with profiles
- ✅ **Payments** - Transaction interface
- ✅ **Proposals** - Create and vote
- ✅ **Settings** - Configure preferences

### Backend
- ✅ **Daemon** - P2P networking, gossip
- ✅ **Gateway** - API + WebSocket server
- ✅ **Ledger** - Mutual credit accounting
- ✅ **Governance** - Democratic proposals
- ✅ **Trust** - Web-of-participation
- ✅ **Identity** - DID + SDIS verification

## 📚 Documentation

| File | Description |
|------|-------------|
| `MOBILE_WALLET_INTEGRATION.md` | Complete mobile wallet docs |
| `FULL_STACK_DEPLOY.md` | Deployment guide |
| `SESSION_COMPLETE_20251212.md` | Integration session summary |
| `docs/ARCHITECTURE.md` | System architecture |
| `docs/GETTING_STARTED.md` | Development setup |
| `ROADMAP.md` | Future plans |

## 🧪 Testing Checklist

### Mobile App
- [ ] Login with coop ID
- [ ] View balance
- [ ] Send payment
- [ ] Receive via QR
- [ ] Scan QR code
- [ ] View proposals
- [ ] Cast vote
- [ ] View identity
- [ ] Verify member
- [ ] Attest trust
- [ ] Test offline mode

### Web UI
- [ ] Login
- [ ] View dashboard
- [ ] Browse members
- [ ] Send payment
- [ ] Create proposal
- [ ] Vote on proposal

### Backend
- [ ] Health check: `curl http://localhost:3030/v1/health`
- [ ] WebSocket: `wscat -c ws://localhost:3030/v1/ws`
- [ ] Create coop: `POST /v1/coop`
- [ ] Make payment: `POST /v1/ledger/{coop}/payment`

## 🚨 Troubleshooting

### Mobile App won't connect
1. Check `GATEWAY_URL` in `src/client.ts`
2. Verify gateway is running: `curl http://localhost:3030/v1/health`
3. Check network permissions in app

### Docker services won't start
1. Check ports: `lsof -i :3030` (gateway), `lsof -i :5173` (pilot-ui)
2. Check logs: `docker-compose logs -f`
3. Rebuild: `docker-compose up --build -d`

### Tests failing
1. Clean build: `cargo clean && cargo build`
2. Check deps: `cargo update`
3. Run single test: `cargo test test_name -- --nocapture`

## 🎯 Quick Commands

```bash
# Start full stack
docker-compose -f docker-compose.full.yml up -d

# Mobile dev
cd sdk/react-native/examples/CoopWallet && npm start

# Web dev
cd web/pilot-ui && npm run dev

# Backend dev
cd icn && cargo run --bin icnd

# Run tests
cargo test

# View logs
docker-compose logs -f

# Stop everything
docker-compose down
```

## 📞 Support

- **Docs**: `/docs/`
- **Issues**: GitHub Issues
- **Email**: dev@icn.coop

---

**Everything is wired and ready to go!** 🎉
