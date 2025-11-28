# Treasurer's Guide

A comprehensive guide for cooperative treasurers managing the timebank ledger and financial health.

## Your Role as Treasurer

As treasurer, you're responsible for:

- **Monitoring** the overall health of the timebank economy
- **Reporting** on transaction volumes and member participation
- **Identifying** imbalances that might indicate problems
- **Recommending** policy changes to the governance body
- **Auditing** the ledger for accuracy and fairness

You have the same access as regular members (the timebank is transparent), but you bring financial oversight skills.

## Weekly Tasks

### 1. Review Transaction Volume

**Goal**: Ensure the timebank is active and healthy

**Steps**:
1. Go to **History** tab
2. Select **"This Week"** filter
3. Check the **Monthly Hours** stat on dashboard

**Healthy Indicators**:
- Steady or increasing weekly volume
- Multiple members transacting
- Variety of services being exchanged

**Warning Signs**:
- Zero transactions for multiple weeks
- Only 2-3 members trading
- Same service repeated (possible gaming)

### 2. Export Weekly Report

**Steps**:
1. Go to **History** tab
2. Select **"This Week"** filter
3. Select **"Newest First"** sort
4. Click **"Export CSV"**
5. Open in Excel/Google Sheets

**Analyze**:
- Total volume (sum of amounts)
- Number of unique participants (count unique DIDs)
- Average transaction size
- Most active members
- Most common services (from memo field)

**Template Spreadsheet**:
```
Transaction Analysis - Week of [Date]
=====================================
Total Transactions: =COUNTA(A:A)
Total Hours Exchanged: =SUM(E:E)
Unique Members: =unique count of columns C and D
Average Transaction: =AVERAGE(E:E)
Top 3 Members: =sort and filter
```

### 3. Check Member Balances

**Goal**: Identify members who may need support

**Steps**:
1. Go to **Members** tab
2. Check for extreme balances (highly positive or negative)
3. Note members with **red (negative)** balances

**Balance Health**:
- **Healthy**: Most members between -50 and +50 hours
- **Concerning**: Members beyond ±100 hours
- **Critical**: Members beyond ±200 hours

**Action Items**:
- Members with large negative balances may need:
  - Skills training to contribute more
  - Adjustment of credit limits (ask admin)
  - Dispute resolution (if fraud suspected)
- Members with large positive balances may:
  - Not be asking for help (encourage them!)
  - Be offering in-demand skills (good sign)
  - Need more variety of services available

### 4. Review Dashboard Stats

**Weekly Snapshot**:
- **Total Members**: Should grow over time
- **Monthly Hours**: Should increase or stay steady
- **Recent Activity**: Should show diverse exchanges

## Monthly Tasks

### 1. Generate Monthly Report

**Steps**:
1. Go to **History** tab
2. Select **"This Month"** filter
3. Click **"Export CSV"**
4. Create report with:

**Key Metrics**:
```
Monthly Timebank Report - [Month Year]
=======================================

Participation:
- Total Active Members: [X] (logged at least 1 transaction)
- Total Members: [Y] (including inactive)
- Participation Rate: [X/Y%]

Volume:
- Total Hours Exchanged: [Z]
- Total Transactions: [N]
- Average Transaction Size: [Z/N]

Balance Health:
- Members in Balance (-10 to +10): [count]
- Members with Debt (< -10): [count] (list if > -50)
- Members with Credit (> +10): [count] (list if > +50)
- Total System Balance: [should be 0]

Top Services (by volume):
1. [Service]: [X hours]
2. [Service]: [X hours]
3. [Service]: [X hours]

Trends:
- Volume vs Last Month: [+/-X%]
- New Members This Month: [N]
- Inactive Members (no activity 3+ months): [N]
```

### 2. Verify Ledger Integrity

**Double-Entry Check**: The total of all balances should always equal **zero** (every debit has a matching credit).

**Steps**:
1. Export **"All Time"** transactions
2. For each member, calculate: `SUM(received) - SUM(given)`
3. Sum all member balances
4. Result should be exactly **0.0**

**If not zero**:
- Contact system administrator immediately
- There may be a ledger corruption issue
- Do NOT approve new transactions until resolved

### 3. Identify Inactive Members

**Goal**: Re-engage or remove inactive accounts

**Steps**:
1. Export **"All Time"** transactions
2. Filter for members with no transactions in last 90 days
3. Create list of inactive members

**Action Items**:
- Email inactive members to check in
- Offer skills training or onboarding help
- Recommend removal to governance if truly inactive (keeps data clean)

### 4. Review Proposal Activity

**Goal**: Track governance participation

**Steps**:
1. Go to **Governance** tab
2. Review proposals created this month
3. Check vote participation rates

**Healthy Indicators**:
- Proposals get >50% member participation
- Decisions are made within 2 weeks
- Mix of passed and failed proposals (shows real debate)

## Quarterly Tasks

### 1. Economic Health Report

**Comprehensive Analysis**:

**A. Velocity of Money**
- How fast are credits circulating?
- Formula: `Total Hours Exchanged / Average Balance`
- **Healthy**: 4-8 (money changes hands 4-8 times per period)
- **Concerning**: <2 (hoarding) or >10 (inflation risk)

**B. Gini Coefficient**
- Measure of inequality (0 = perfect equality, 1 = perfect inequality)
- Use Excel/Google Sheets: [Gini formula](https://en.wikipedia.org/wiki/Gini_coefficient)
- **Healthy**: 0.2-0.4
- **Concerning**: >0.5 (wealth concentration)

**C. Participation Distribution**
- What % of members are responsible for what % of volume?
- **Healthy**: 80/20 rule (80% of volume from 20% of members is normal)
- **Concerning**: 95/5 rule (tiny minority doing all trading)

**D. Service Diversity**
- Count unique services from memo field
- **Healthy**: 10+ different service categories
- **Concerning**: <5 (limited exchange options)

### 2. Policy Recommendations

Based on your analysis, recommend to governance:

**If Hoarding Detected** (high positive balances, low velocity):
- Implement demurrage (negative interest on large balances)
- Create "spend-down" incentives
- Encourage high-balance members to request services

**If Debt Spiral Detected** (many negative balances, increasing):
- Tighten credit limits
- Increase skills training offerings
- Review dispute resolution process

**If Low Participation**:
- Marketing campaign to recruit new members
- Skills inventory to match supply/demand
- Social events to build community trust

**If Service Concentration** (few people, same services):
- Recruit members with different skills
- Cross-training programs
- Service request board

### 3. Audit Report for Governance

**Present to Governance**:
- Quarterly financial summary
- Policy recommendations
- Member concerns or disputes
- System health score (your assessment)

**Template**:
```
Q[X] 20XX Timebank Audit Report
================================

Executive Summary:
[Overall health: Excellent/Good/Fair/Poor]

Key Metrics:
- Total Hours Exchanged: [X] (±Y% from last quarter)
- Active Member Rate: [X%]
- Average Balance: [X] hours
- Velocity: [X] rotations/quarter

Concerns:
1. [Issue 1]
2. [Issue 2]

Recommendations:
1. [Action 1]
2. [Action 2]

Detailed Data:
[Attach CSV exports and analysis spreadsheets]
```

## Annual Tasks

### 1. Year-End Financial Summary

**Comprehensive Annual Report**:

**Include**:
- Total hours exchanged (entire year)
- Growth metrics (members, volume, diversity)
- Top contributors (by volume)
- Success stories (interviews with active members)
- Challenges overcome
- Goals for next year

**Present at Annual Meeting**:
- Visual charts (use Excel/Sheets to create graphs)
- Comparison to previous year
- Impact stories (qualitative data)

### 2. Data Archival

**Steps**:
1. Export **"All Time"** transactions
2. Save CSV with filename: `timebank-archive-[YEAR].csv`
3. Store securely (encrypted backup)
4. Document any system changes or migrations

### 3. Strategic Planning

**Questions to Answer**:
- Is the timebank growing sustainably?
- What services are most in-demand?
- What barriers prevent participation?
- How can we increase diversity of services?
- What technology improvements are needed?

## Understanding Transactions

### Transaction Anatomy

Every transaction has:
- **From DID**: Person providing service
- **To DID**: Person receiving service
- **Amount**: Hours of service
- **Currency**: Usually "hours"
- **Timestamp**: When logged
- **Memo**: Description of service

**Key Insight**: When Alice helps Bob:
- Alice's balance **increases** (she gave, Bob owes her)
- Bob's balance **decreases** (he received, he owes Alice)

### Common Transaction Patterns

**Reciprocal Pairs**:
```
Alice → Bob: 2 hours (garden work)
Bob → Alice: 2 hours (computer repair)
```
Both balances return to previous levels. Healthy!

**One-Way Flow**:
```
Alice → Bob: 5 hours
Alice → Carol: 3 hours
Alice → Dave: 2 hours
```
Alice builds up 10 hours credit but never requests help. May indicate:
- Alice is generous (good!)
- Alice doesn't trust system (concerning)
- Alice doesn't need help (rare)
- Alice doesn't know how to ask (opportunity for outreach)

**Debt Spiral**:
```
Bob ← Alice: 5 hours
Bob ← Carol: 5 hours
Bob ← Dave: 5 hours
```
Bob receives 15 hours of help but provides nothing. May indicate:
- Bob has no skills to offer (needs training)
- Bob is gaming the system (needs investigation)
- Bob is in crisis (needs support, may warrant write-off)

## Export and Analysis

### CSV Export Tips

**Best Practices**:
1. Use **consistent filter periods** for trend comparison
2. Always select **"All Time"** for full audits
3. Export **at the same time each period** (e.g., 9am Monday)
4. **Name files clearly**: `transactions-month-2024-01.csv`

**CSV Structure**:
```
Date,Time,From,To,Amount,Currency,Memo
11/20/2024,3:45 PM,did:icn:alice,did:icn:bob,2.5,hours,"Garden weeding"
```

### Excel/Google Sheets Formulas

**Sum all hours**:
```
=SUM(E:E)
```

**Count unique participants**:
```
=COUNTA(UNIQUE(C:C))+COUNTA(UNIQUE(D:D))
```

**Filter by member**:
```
=FILTER(A:G, OR(C:C="did:icn:alice", D:D="did:icn:alice"))
```

**Calculate member balance**:
```
=SUMIF(D:D,"did:icn:alice",E:E) - SUMIF(C:C,"did:icn:alice",E:E)
```
(Received minus given)

**Pivot table for service breakdown**:
1. Select all data
2. Insert > Pivot Table
3. Rows: Memo field
4. Values: Sum of Amount

### Data Visualization

**Create Charts**:

**Transaction Volume Over Time**:
- Line chart
- X-axis: Date
- Y-axis: Cumulative hours

**Member Balance Distribution**:
- Histogram
- Bins: -200 to +200 in increments of 20
- Shows if balances are centered around zero

**Service Categories**:
- Pie chart or bar chart
- Breakdown by service type from memo field

## Red Flags to Watch For

### Fraud Indicators

**Pattern**: Same two members trading large amounts back-and-forth
**Risk**: Circular trading to inflate balances
**Action**: Interview members, review memos for legitimacy

**Pattern**: Member with huge positive balance suddenly transfers to new member
**Risk**: Account sale or transfer (violates timebank principles)
**Action**: Investigate, may warrant account suspension

**Pattern**: Many transactions with identical timestamps
**Risk**: Backdated or fake transactions
**Action**: Check WebSocket logs, verify with members

**Pattern**: Transaction amounts that don't make sense (e.g., 100 hours for "coffee chat")
**Risk**: Balance manipulation
**Action**: Require justification, adjust credit limits

### System Issues

**Pattern**: Total balances don't sum to zero
**Risk**: Ledger corruption or software bug
**Action**: Stop all transactions, contact developer immediately

**Pattern**: Duplicate transactions
**Risk**: Double-spend bug
**Action**: Identify duplicates, reverse one, investigate cause

**Pattern**: Transactions from members not in member list
**Risk**: Data integrity issue
**Action**: Verify member existence, check access controls

## Dispute Resolution

### When Members Disagree

**Common Disputes**:
1. "I didn't receive the service"
2. "The work was poor quality"
3. "They logged more hours than actually worked"
4. "I was charged for something I didn't authorize"

**Your Role**:
1. **Listen** to both parties
2. **Review** the transaction records
3. **Mediate** a fair solution
4. **Recommend** to governance if you can't resolve

**Tools**:
- Export transactions for both members
- Review memo field for details
- Check if this is a pattern (repeat disputes)

**Resolution Options**:
1. **Reverse transaction**: Admin can undo (for clear errors)
2. **Partial credit**: Compromise amount
3. **Write-off**: Forgive debt (for hardship cases)
4. **Suspension**: Temporary ban (for fraud)
5. **Expulsion**: Governance decision (extreme cases)

### Documenting Disputes

**Keep Records**:
```
Dispute Log Entry
=================
Date: [Date]
Parties: [Member A] vs [Member B]
Transaction: [Date, Amount, Memo]
Issue: [Description]
Evidence: [Emails, screenshots, witness statements]
Resolution: [Action taken]
Follow-up: [Any ongoing monitoring]
```

## Reporting to Governance

### Monthly Treasurer Report Template

```
Treasurer's Report - [Month Year]
==================================

1. Transaction Summary
   - Total Hours: [X]
   - Total Transactions: [X]
   - Active Members: [X] of [Y] ([Z%])

2. Balance Health
   - Average Balance: [X] hours
   - Members in Debt (< -10): [X]
   - Members with Credit (> +10): [X]
   - Largest Debt: [X] hours
   - Largest Credit: [X] hours

3. System Integrity
   ✅ Ledger balances sum to zero
   ✅ No duplicate transactions detected
   ✅ All transactions have valid DIDs

4. Notable Activity
   - [Any unusual patterns or concerns]

5. Recommendations
   - [Policy suggestions based on data]

6. Disputes This Month
   - [Summary of any disputes and resolutions]

7. Attachments
   - Monthly transaction export (CSV)
   - Balance distribution chart
```

## Tools and Resources

### Recommended Software

- **Spreadsheet**: Excel, Google Sheets, LibreOffice Calc
- **Backup**: Encrypted cloud storage (Google Drive, Dropbox)
- **Reporting**: Google Data Studio for interactive dashboards
- **Communication**: Email lists for member outreach

### Key Metrics Cheat Sheet

| Metric | Healthy Range | Warning Signs |
|--------|---------------|---------------|
| Velocity | 4-8 rotations/quarter | <2 or >10 |
| Participation Rate | >50% active | <30% active |
| Average Balance | ±25 hours | ±100 hours |
| Gini Coefficient | 0.2-0.4 | >0.5 |
| Transaction Frequency | >2/week | <1/month |
| Service Diversity | >10 categories | <5 categories |

### Further Reading

- [Community Currencies in Action](https://www.complementarycurrency.org/)
- [Timebanking UK Resources](https://timebanking.org/)
- [Cooperative Economics 101](https://www.ica.coop/)

## FAQ for Treasurers

**Q: How often should I check the ledger?**
A: Weekly spot-checks, monthly deep dive, quarterly comprehensive audit.

**Q: What if a member requests their balance be adjusted?**
A: Only admins can adjust balances. Refer to governance policy. Adjustments should be rare and well-documented.

**Q: Can I see private transaction details?**
A: No, memos are visible to all members. The ledger is transparent by design. This builds trust.

**Q: What's a "healthy" timebank size?**
A: 20-100 active members is manageable. Below 20, not enough diversity. Above 100, consider splitting into sub-groups.

**Q: How do I handle deceased or departed members?**
A: Governance should have a policy. Options: write off balance, transfer to family, donate to community fund.

**Q: Should we charge fees?**
A: Most timebanks don't charge fees (violates mutual aid principles). If needed, use separate payment system, not timebank credits.

## Getting Help

**Technical Issues**:
- Check the [Admin Guide](ADMIN-GUIDE.md) for system administration
- Report bugs at https://github.com/InterCooperative-Network/icn/issues

**Economic Questions**:
- Consult your governance body
- Reach out to other timebank treasurers
- Read case studies from established timebanks

**Training**:
- Ask experienced treasurers to mentor you
- Attend cooperative finance workshops
- Study cooperative economics resources

---

**Remember**: Your role is to ensure the timebank economy is fair, transparent, and sustainable. You're not just tracking numbers—you're stewarding a community's trust.

Good luck! 📊
