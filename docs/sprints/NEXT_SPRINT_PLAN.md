# ICN Next Sprint: Production Hardening & Deployment

**Sprint Goal**: Transform pilot features into production-ready systems
**Duration**: 2-3 weeks
**Start Date**: December 16, 2025
**Target Launch**: Early January 2026

---

## Sprint Objectives

1. **Replace prototypes with production implementations**
2. **Add persistent storage for all economic features**
3. **Complete notification delivery infrastructure**
4. **Implement payment execution logic**
5. **Add comprehensive monitoring**
6. **Prepare for pilot cooperative launch**

---

## Phase 1: Storage Layer (Week 1)

### Priority: P0 (Critical)
**Goal**: Replace in-memory stores with persistent, crash-safe storage

#### Task 1.1: Recurring Payments Storage
**Estimate**: 6-8 hours

- [ ] Create Sled-based `RecurringPaymentStore`
- [ ] Schema: payment schedules, execution history
- [ ] Migration from in-memory prototype
- [ ] Add indices for due date queries
- [ ] Implement backup/restore
- [ ] Test crash recovery

**Files**:
- `icn-store/src/recurring_payments.rs`
- `icn-store/src/migrations/005_recurring_payments.rs`

#### Task 1.2: Escrow Storage
**Estimate**: 6-8 hours

- [ ] Create `EscrowStore` with Sled backend
- [ ] Schema: escrows, conditions, approvals
- [ ] Support for conditional queries
- [ ] Transaction log for releases/refunds
- [ ] Audit trail for compliance

**Files**:
- `icn-store/src/escrow.rs`
- `icn-store/src/migrations/006_escrow.rs`

#### Task 1.3: Budget Storage
**Estimate**: 6-8 hours

- [ ] Create `BudgetStore` with period tracking
- [ ] Schema: budgets, spending history
- [ ] Efficient period reset mechanism
- [ ] Notification threshold tracking
- [ ] Historical reporting queries

**Files**:
- `icn-store/src/budgets.rs`
- `icn-store/src/migrations/007_budgets.rs`

#### Task 1.4: Notification Persistence
**Estimate**: 4-6 hours

- [ ] Persist in-app notifications
- [ ] Delivery status tracking
- [ ] Failed delivery queue
- [ ] Retention policy (30 days)
- [ ] Cleanup job

**Files**:
- `icn-store/src/notifications.rs`

---

## Phase 2: Notification Delivery (Week 1)

### Priority: P0 (Critical)
**Goal**: Complete email and push notification delivery

#### Task 2.1: SMTP Email Delivery
**Estimate**: 8-10 hours

- [ ] Integrate `lettre` crate
- [ ] Configure SMTP connection pool
- [ ] Implement template rendering (Handlebars)
- [ ] Create templates for all notification types:
  - Payment received/sent
  - Amendment voting opened
  - Appeal filed
  - Budget threshold reached
- [ ] HTML + plain text versions
- [ ] Add inline CSS for email clients
- [ ] Test with major email providers
- [ ] Implement rate limiting

**Dependencies**: `lettre = "0.11"`, `handlebars = "5.0"`

**Files**:
- `icn-gateway/src/email_delivery.rs`
- `icn-gateway/templates/email/*.hbs`

#### Task 2.2: FCM Push Notifications
**Estimate**: 6-8 hours

- [ ] Complete OAuth2 JWT signing
- [ ] Implement token refresh logic
- [ ] Add device token management
- [ ] Handle FCM errors (invalid tokens, etc.)
- [ ] Implement notification priority
- [ ] Add notification actions (deep links)
- [ ] Test on Android/iOS

**Dependencies**: `jsonwebtoken = "9.0"`

**Files**:
- `icn-gateway/src/fcm_delivery.rs`

#### Task 2.3: Delivery Status Tracking
**Estimate**: 4-6 hours

- [ ] Track delivery success/failure per channel
- [ ] Retry failed deliveries
- [ ] Dead letter queue for permanent failures
- [ ] Delivery metrics (Prometheus)
- [ ] Admin dashboard for monitoring

**Files**:
- `icn-gateway/src/delivery_tracker.rs`

---

## Phase 3: Payment Execution (Week 1-2)

### Priority: P0 (Critical)
**Goal**: Execute scheduled and conditional payments

#### Task 3.1: Recurring Payment Scheduler
**Estimate**: 10-12 hours

- [ ] Background scheduler service
- [ ] Query due payments every minute
- [ ] Execute payment via ledger
- [ ] Update next execution timestamp
- [ ] Handle execution failures:
  - Insufficient balance
  - Invalid accounts
  - Network errors
- [ ] Retry logic with exponential backoff
- [ ] Completion detection (end date reached)
- [ ] Notification on execution/failure

**Dependencies**: `tokio-cron-scheduler = "0.9"`

**Files**:
- `icn-gateway/src/payment_scheduler.rs`
- `icn-gateway/src/payment_executor.rs`

#### Task 3.2: Escrow Fund Management
**Estimate**: 8-10 hours

- [ ] Lock funds when escrow created
- [ ] Verify locked funds on ledger
- [ ] Release funds to beneficiary
- [ ] Refund funds to sender
- [ ] Handle partial releases
- [ ] Expiration handling
- [ ] Audit logging for all operations

**Files**:
- `icn-gateway/src/escrow_manager.rs`

#### Task 3.3: Budget Enforcement
**Estimate**: 6-8 hours

- [ ] Hook into ledger transaction flow
- [ ] Check budget before transaction
- [ ] Update spent amount after transaction
- [ ] Trigger threshold notifications
- [ ] Block transactions when exceeded
- [ ] Admin override capability
- [ ] Reporting for budget usage

**Files**:
- `icn-ledger/src/budget_enforcer.rs`

---

## Phase 4: Monitoring & Observability (Week 2)

### Priority: P1 (High)
**Goal**: Production-ready monitoring and alerting

#### Task 4.1: Prometheus Metrics
**Estimate**: 6-8 hours

- [ ] Notification metrics:
  - Delivery success rate by channel
  - Queue depth
  - Processing latency
  - Retry counts
- [ ] Payment metrics:
  - Recurring payment executions
  - Execution failures
  - Escrow operations
  - Budget breaches
- [ ] API metrics:
  - Request rate by endpoint
  - Error rates
  - Response times

**Files**:
- `icn-obs/src/notification_metrics.rs`
- `icn-obs/src/payment_metrics.rs`

#### Task 4.2: Structured Logging
**Estimate**: 4-6 hours

- [ ] JSON structured logging
- [ ] Request ID tracing
- [ ] Log levels per component
- [ ] PII redaction in logs
- [ ] Log aggregation setup (Loki/ELK)

**Files**:
- `icn-obs/src/structured_logger.rs`

#### Task 4.3: Health Checks
**Estimate**: 3-4 hours

- [ ] Detailed health endpoint
- [ ] Component status:
  - Database connectivity
  - SMTP connection
  - FCM API reachability
  - Queue health
- [ ] Readiness vs liveness probes
- [ ] Graceful degradation indicators

**Files**:
- `icn-gateway/src/health_detailed.rs`

---

## Phase 5: Client SDK Implementation (Week 2)

### Priority: P1 (High)
**Goal**: Working TypeScript and React Native SDKs

#### Task 5.1: TypeScript SDK Core
**Estimate**: 12-16 hours

- [ ] Implement all API clients:
  - Notifications
  - Recurring payments
  - Escrow
  - Budgets
  - Governance (voting, appeals, dashboard)
- [ ] WebSocket client with auto-reconnect
- [ ] Token refresh logic
- [ ] Error handling and retries
- [ ] Request/response logging
- [ ] Type generation from OpenAPI
- [ ] Unit tests

**Files**:
- `sdk/typescript/src/clients/*.ts`
- `sdk/typescript/src/websocket.ts`

#### Task 5.2: React Native Hooks
**Estimate**: 10-12 hours

- [ ] Implement all hooks:
  - `useNotifications()`
  - `useRecurringPayments()`
  - `useEscrows()`
  - `useBudgets()`
  - `useAmendmentVoting()`
  - `useGovernanceDashboard()`
- [ ] Local state management (React Query/SWR)
- [ ] Optimistic updates
- [ ] Error boundaries
- [ ] Loading states
- [ ] Offline support

**Files**:
- `sdk/react-native/src/hooks/*.ts`

#### Task 5.3: Push Notification Integration
**Estimate**: 6-8 hours

- [ ] FCM setup guide
- [ ] Device registration helper
- [ ] Notification permission flow
- [ ] Foreground/background handlers
- [ ] Notification tap handling
- [ ] Deep linking setup

**Files**:
- `sdk/react-native/src/push-notifications.ts`

---

## Phase 6: Testing & Documentation (Week 2-3)

### Priority: P1 (High)

#### Task 6.1: End-to-End Tests
**Estimate**: 10-12 hours

- [ ] Test scenarios:
  - User creates recurring payment → executed on schedule
  - User creates escrow → approved → released
  - User exceeds budget → blocked → admin override
  - User receives notification → marks read
  - User votes on amendment → results update
- [ ] Use TestContainers for dependencies
- [ ] Parallel test execution
- [ ] CI/CD integration

**Files**:
- `icn/tests/e2e/*.rs`

#### Task 6.2: Load Testing
**Estimate**: 6-8 hours

- [ ] Notification queue under load (1k/sec)
- [ ] Payment scheduler with 10k active schedules
- [ ] Concurrent budget checks
- [ ] WebSocket connections (1k concurrent)
- [ ] Identify bottlenecks
- [ ] Performance tuning

**Tools**: `k6`, `vegeta`, `wrk`

#### Task 6.3: Security Audit
**Estimate**: 8-10 hours

- [ ] Input validation review
- [ ] SQL injection prevention (once DB added)
- [ ] XSS in email templates
- [ ] Authorization bypass attempts
- [ ] Rate limiting effectiveness
- [ ] Secret management review
- [ ] Dependency audit (`cargo audit`)

#### Task 6.4: Deployment Documentation
**Estimate**: 6-8 hours

- [ ] Production deployment guide
- [ ] Configuration reference
- [ ] Secret management (Vault/SOPS)
- [ ] Database migrations guide
- [ ] Backup/restore procedures
- [ ] Monitoring setup guide
- [ ] Troubleshooting runbook

**Files**:
- `docs/deployment/PRODUCTION.md`
- `docs/deployment/CONFIGURATION.md`
- `docs/deployment/MONITORING.md`
- `docs/deployment/TROUBLESHOOTING.md`

---

## Phase 7: Pilot Preparation (Week 3)

### Priority: P2 (Medium)
**Goal**: Ready for real cooperative pilot

#### Task 7.1: Pilot Cooperative Setup
**Estimate**: 4-6 hours

- [ ] Charter creation script
- [ ] Founder onboarding flow
- [ ] Initial budget allocation
- [ ] Test notification delivery
- [ ] Sample amendment creation
- [ ] Governance dashboard validation

#### Task 7.2: User Onboarding Guide
**Estimate**: 4-6 hours

- [ ] Welcome email templates
- [ ] Getting started guide
- [ ] Mobile app installation
- [ ] Feature walkthroughs
- [ ] FAQ document
- [ ] Support contact info

**Files**:
- `docs/pilot/ONBOARDING.md`
- `docs/pilot/USER_GUIDE.md`

#### Task 7.3: Admin Training
**Estimate**: 3-4 hours

- [ ] Admin dashboard walkthrough
- [ ] Common administrative tasks
- [ ] Emergency procedures
- [ ] Monitoring dashboards
- [ ] Incident response guide

---

## Success Criteria

### Must Have (P0)
- ✅ All data persisted to disk (no in-memory stores)
- ✅ Email delivery working with real SMTP
- ✅ Push notifications working on iOS/Android
- ✅ Recurring payments executing on schedule
- ✅ Escrow funds locked and released correctly
- ✅ Budget enforcement blocking transactions
- ✅ Prometheus metrics exposed
- ✅ All tests passing (unit + integration + e2e)

### Should Have (P1)
- ✅ TypeScript SDK fully functional
- ✅ React Native hooks implemented
- ✅ Comprehensive documentation
- ✅ Load testing completed
- ✅ Security audit complete

### Nice to Have (P2)
- Admin UI for monitoring
- Automated backup scripts
- Performance dashboards
- User analytics

---

## Risk Management

### High Risk
1. **SMTP delivery issues** - Mitigation: Test with multiple providers, fallback to logging
2. **FCM rate limits** - Mitigation: Implement batching, respect FCM quotas
3. **Payment execution failures** - Mitigation: Robust retry logic, manual intervention capability

### Medium Risk
1. **Database migration complexity** - Mitigation: Thorough testing, rollback procedures
2. **Load testing reveals bottlenecks** - Mitigation: Identify early, optimize hot paths
3. **Mobile SDK bugs** - Mitigation: Example app for testing, beta release

### Low Risk
1. **Documentation gaps** - Mitigation: Iterative writing alongside implementation
2. **Monitoring setup delays** - Mitigation: Start with basics, iterate

---

## Timeline

### Week 1 (Dec 16-22)
- **Mon-Tue**: Storage layer (Tasks 1.1-1.4)
- **Wed-Thu**: Email delivery (Task 2.1)
- **Fri-Sat**: Push notifications (Task 2.2)
- **Sun**: Buffer/catchup

### Week 2 (Dec 23-29)
- **Mon-Tue**: Payment scheduler (Task 3.1)
- **Wed**: Escrow management (Task 3.2)
- **Thu**: Budget enforcement (Task 3.3)
- **Fri-Sat**: Monitoring (Tasks 4.1-4.3)
- **Sun**: Holiday break

### Week 3 (Dec 30 - Jan 5)
- **Mon-Tue**: TypeScript SDK (Task 5.1)
- **Wed**: React Native hooks (Task 5.2)
- **Thu**: Testing (Tasks 6.1-6.2)
- **Fri**: Documentation (Task 6.4)
- **Sat-Sun**: Pilot prep (Tasks 7.1-7.3)

---

## Resources Required

### Development
- Rust toolchain (already installed)
- Docker for test dependencies
- SMTP server access (SendGrid/Mailgun)
- FCM service account credentials

### Infrastructure
- Kubernetes cluster (or VPS)
- Prometheus + Grafana
- Log aggregation (Loki/ELK)
- Backup storage (S3/B2)

### External Services
- Email delivery service ($10-50/month)
- Firebase account (free tier)
- Domain + SSL certificates

---

## Post-Sprint

### Immediate Follow-up
1. Pilot launch with 1-2 cooperatives
2. Gather user feedback
3. Bug fixes and UX improvements
4. Performance optimization

### Future Sprints
1. **Mobile App Polish** - Native UI components
2. **Analytics & Reporting** - Usage dashboards
3. **Advanced Features** - Proposal templates, voting delegation
4. **Federation** - Cross-cooperative payments
5. **Compliance** - KYC/AML if required

---

## Team Notes

This sprint is **critical** for moving from prototype to production. Focus on:
- **Reliability**: No data loss, graceful error handling
- **Observability**: Can diagnose issues quickly
- **Security**: No shortcuts on authorization/authentication
- **Documentation**: Future maintainers can understand the system

**Estimated Total Effort**: 120-150 hours (~3 weeks single developer)

---

**Sprint Status**: 📋 PLANNED
**Next Action**: Begin Task 1.1 (Recurring Payments Storage)
**Review Date**: Every Friday for progress check
