# Frequently Asked Questions (FAQ)

Common questions about using the ICN timebank pilot.

## Table of Contents

- [Getting Started](#getting-started)
- [Authentication & Security](#authentication--security)
- [Using the Timebank](#using-the-timebank)
- [Understanding Balances](#understanding-balances)
- [Transactions](#transactions)
- [Governance](#governance)
- [Technical Issues](#technical-issues)
- [Philosophy & Principles](#philosophy--principles)

---

## Getting Started

### What is a timebank?

A timebank is a system where people exchange services using time as currency. Instead of paying money, you "pay" with your time. When you help someone for 1 hour, you earn 1 time credit that you can use to receive 1 hour of help from anyone else in the community.

**Key principle**: Everyone's time is valued equally, regardless of the service provided. An hour of garden work equals an hour of computer repair equals an hour of childcare.

### What is ICN?

ICN (Intercooperative Network) is the software platform that powers this timebank. It's open-source, peer-to-peer, and designed specifically for cooperative communities.

### Do I need technical skills to use the timebank?

No! The web interface is designed to be user-friendly. You'll need to:
- Use a web browser
- Get an authentication token (we provide step-by-step instructions)
- Know how to copy and paste

That's it. Your admin handles the technical setup.

### How do I sign up?

1. Your cooperative administrator adds you to the system
2. You receive a welcome email with your credentials
3. You create your identity using the `icnctl` tool (detailed instructions provided)
4. You sign in to the web interface

See the [Quick Start Guide](QUICK-START.md) for detailed steps.

### What browsers are supported?

The timebank works on:
- Chrome 90+
- Firefox 88+
- Safari 14+
- Edge 90+

Both desktop and mobile browsers are supported.

---

## Authentication & Security

### What is a DID?

DID stands for "Decentralized Identifier." It's your unique ID in the system, like a username but cryptographically secured. It looks like: `did:icn:abc123xyz789...`

Think of it as your timebank "account number" that only you control.

### What is an authentication token?

An authentication token is like a temporary password that proves you own your DID. You get it by running a command that signs a challenge with your private key.

**Key facts**:
- Tokens expire after 24 hours (for security)
- You can generate new tokens anytime
- Never share your token (treat it like a password)

### How do I get a token?

Click the **"How do I get a token?"** button on the login screen. Follow the 3-step wizard:

1. Open your terminal
2. Run the provided command
3. Copy the token back into the login form

The command looks like:
```bash
icnctl auth login --gateway http://localhost:8080 --coop your-coop-id
```

### Why does my token expire?

Tokens expire after 24 hours for security. If your token was stolen, it would only be valid for a short time. This is standard practice for secure systems.

You'll see warnings at 15, 10, and 5 minutes before expiration so you can get a new token.

### What if I forget my passphrase?

Your passphrase unlocks your keystore (your private identity). If you forget it:

1. **If you have a backup**: Restore from your backed-up keystore
2. **If you don't have a backup**: You'll need to create a new identity and have your admin add the new DID

**Prevention**: Back up your keystore when you first create it!

```bash
icnctl id export my-backup.age
```

Store `my-backup.age` somewhere safe (encrypted USB drive, password manager, etc.).

### Is my data private?

**What's public** (visible to all members):
- Your DID
- Your transactions (amounts, who you traded with, when, what service)
- Your balance
- Your votes on proposals

**What's private**:
- Your keystore (private key)
- Your passphrase
- Your authentication tokens

The timebank is transparent by design—this builds trust. If you need privacy for a specific exchange, handle it outside the timebank and just log a generic memo like "Consulting services."

### How secure is the system?

ICN uses multiple layers of security:
- **Ed25519 cryptography** for signatures (same as SSH, Signal)
- **QUIC/TLS** for network encryption
- **JWT tokens** with expiration for authentication
- **Rate limiting** to prevent abuse
- **Audit logs** for accountability

No system is 100% secure, but ICN follows industry best practices.

---

## Using the Timebank

### How do I log a service I provided?

1. Click **"Log Hours"** tab
2. Select **who you helped** from the dropdown
3. Enter **number of hours** (e.g., 2.5 for 2 hours 30 minutes)
4. Add a **description** (e.g., "Garden weeding")
5. Click **"Log Hours"**

✅ Your balance increases, their balance decreases (they now owe you).

### Can I log services in increments less than 1 hour?

Yes! You can log in 15-minute increments (0.25 hours):
- 15 minutes = 0.25
- 30 minutes = 0.5
- 45 minutes = 0.75
- 1 hour 30 minutes = 1.5

### Who logs the transaction—the giver or receiver?

**Typically the giver** (the person who provided the service). This makes sense because:
- You're claiming the credit you earned
- You control the description

However, your cooperative may have different norms. Check with your admin.

### Can I edit or delete a transaction?

**No.** The ledger is immutable (permanent) by design. This prevents fraud and maintains an honest history.

If you made an error:
1. Contact the other person involved
2. Log a correction transaction (e.g., reverse the amount)
3. If it's a dispute, involve your treasurer or governance

### How do I search for a specific member?

1. Go to **Members** tab
2. Type in the **search box** at the top
3. Results filter as you type

You can search by any part of their DID.

### Can I see someone's transaction history?

No, you can only see:
- Transactions you're involved in (sent or received)
- Transactions shown in "Recent Activity" (last 5, for everyone)

For privacy reasons, full histories are not exposed. However, your treasurer may have tools to generate reports.

### How do I export my transactions?

1. Go to **History** tab
2. Select the **time period** you want (Today, This Week, This Month, etc.)
3. Click **"Export CSV"**
4. Open the CSV file in Excel, Google Sheets, or any spreadsheet software

### What are the keyboard shortcuts?

- **Ctrl+1** (Cmd+1 on Mac): Go to Dashboard
- **Ctrl+2**: Go to Log Hours
- **Ctrl+3**: Go to History
- **Ctrl+4**: Go to Members
- **Ctrl+5**: Go to Governance

These work from anywhere in the app!

---

## Understanding Balances

### What does my balance mean?

Your balance represents:
- **Positive (green)**: You've given more than received. Others owe you time.
- **Negative (red)**: You've received more than given. You owe time to the community.
- **Zero**: You've exchanged equally.

### Is a negative balance bad?

**No!** A negative balance means you've received help from your community. That's what the timebank is for!

The goal isn't to hoard credits—it's to exchange. Think of it like breathing: sometimes you inhale (receive), sometimes you exhale (give).

**However**: Very large negative balances (e.g., < -100) may trigger limits to prevent abuse. Check your cooperative's policy.

### Is a positive balance good?

It means you've been generous, which is great! But if your balance is very high (e.g., > +100), ask yourself:
- Am I asking for help when I need it?
- Am I afraid to "spend" my credits?
- Are there services I want but aren't available?

The timebank works best when credits circulate. Don't be afraid to ask for help—that's the whole point!

### What's the arrow next to my balance?

The trend indicator shows:
- **↑ (green)**: Your balance is increasing (you're giving more recently)
- **↓ (red)**: Your balance is decreasing (you're receiving more recently)
- **→ (gray)**: Your balance is stable (balanced exchanges)

It compares your activity in the last 7 days vs. the previous 7 days.

### Can my balance go negative?

Yes. Negative balances are normal and expected. However, your cooperative may set **credit limits** (e.g., -100 hours) to prevent abuse. If you hit your limit, you'll need to provide services before receiving more.

### What happens if I leave the cooperative with a negative balance?

It depends on your cooperative's policy. Common approaches:
- **Write off** (forgive the debt)
- **Partial repayment** (settle what you can)
- **Transfer to community fund** (if it exists)

Talk to your admin or treasurer before leaving.

---

## Transactions

### How long do transactions take to appear?

**Instantly!** After you log hours:
- Both members' balances update immediately
- Transaction appears in Recent Activity
- Notification appears if using WebSocket

The system also auto-refreshes every 30 seconds.

### Why don't I see a transaction I just logged?

Possible reasons:
1. **Wrong filter**: Check that History filter is set to include recent dates (e.g., "This Month" not "Last Year")
2. **Wrong cooperative**: Verify you're logged in to the correct cooperative
3. **WebSocket disconnected**: Check footer status (red dot = disconnected). Refresh the page.
4. **Error occurred**: Check for error message (red toast notification)

If still missing, contact your admin.

### Can I add a note to a transaction?

Yes! The **memo** field is your note. Examples:
- "Garden weeding - south plot"
- "Computer repair - fixed printer"
- "Childcare - 3 hours Tuesday afternoon"

Good memos help with:
- Remembering what you exchanged
- Treasurer reports (what services are popular)
- Resolving disputes ("I said I'd do X, not Y")

### What if someone logged hours incorrectly?

**If it's an honest mistake**:
1. Contact the other person
2. Agree on a correction
3. Log a reverse transaction (e.g., -2 hours if they logged +2 too many)

**If it's fraudulent**:
1. Do NOT log a correction yet
2. Contact your treasurer or administrator
3. File a dispute (if your cooperative has a process)
4. Let governance decide

### How do I filter my transaction history?

Use the two dropdowns in the **History** tab:

**Time Filter**:
- Today (last 24 hours)
- This Week (last 7 days)
- **This Month** (last 30 days - default)
- This Year (last 365 days)
- All Time (everything)

**Sort Order**:
- **Newest First** (default)
- Oldest First
- Highest Amount
- Lowest Amount

### What does "Recent Activity" on the Dashboard show?

The **last 5 transactions** in your entire cooperative (not just yours). This gives you a sense of:
- How active the timebank is
- What services are being exchanged
- Who's participating

To see all your transactions, go to the **History** tab.

---

## Governance

### What are proposals?

Proposals are community decisions put to a vote. Examples:
- Changing cooperative rules
- Approving budgets
- Adding/removing members (formal vote)
- Policy changes

Any member (or your governance process may limit to admins) can create a proposal.

### How do I vote?

1. Go to **Governance** tab
2. Review **Active Proposals**
3. Click **For**, **Against**, or **Abstain** on each

Your vote is recorded immediately. You can see current tallies (for transparency).

### Can I change my vote?

This depends on your cooperative's governance rules. In the current system:
- Votes are typically **final** once cast
- Contact your admin if you need to change a vote (may require governance process)

### What do the proposal statuses mean?

- **Draft**: Created but not yet open for voting
- **Open**: Voting is active
- **Closed**: Voting ended, outcome determined

Only **Open** proposals show vote buttons.

### What does the deadline countdown mean?

Some proposals have a deadline (e.g., "Closes in 2 days"). Colors indicate urgency:
- **Gray**: More than 2 days left
- **Yellow**: 1-2 days left
- **Red**: Less than 24 hours left

Vote before it closes!

### How are outcomes determined?

Common methods (check your cooperative's rules):
- **Simple majority**: >50% of votes are "For"
- **Supermajority**: >66% of votes are "For"
- **Quorum**: Minimum number of members must vote
- **Consensus**: All or nearly all members agree

Your cooperative's governance profile defines this.

### What if I disagree with a decision?

1. **During voting**: Campaign for your position, persuade others
2. **After decision**: Respect the outcome (democratic process)
3. **If strongly opposed**: Propose a new proposal to change it
4. **If fundamentally opposed**: Consider if this cooperative is right for you

Healthy communities have disagreement. The key is respectful debate and accepting democratic outcomes.

---

## Technical Issues

### "Cannot connect to the server"

**Possible causes**:
- Gateway is not running (contact admin)
- Wrong gateway URL (check your welcome email)
- Network issue (check your internet connection)
- Firewall blocking (try from different network)

**Fix**:
1. Verify gateway URL is correct
2. Try `curl http://[gateway-url]/v1/health`
3. Contact your administrator

### "Your session has expired"

Your 24-hour authentication token expired.

**Fix**:
1. Click **Logout**
2. Get a new token: `icnctl auth login --gateway [url] --coop [id]`
3. Sign in again

**Prevention**: Watch the token countdown in the header and refresh before it expires.

### "You don't have permission"

Your token doesn't have the required permissions for this action.

**Possible reasons**:
- You're a "member" trying to do admin actions
- Token was issued with limited scopes
- Cooperative policy restricts this action

**Fix**: Contact your administrator to check your role and permissions.

### "Too many requests"

You're being rate-limited (100 requests per burst).

**Possible causes**:
- Clicking buttons repeatedly (the system is working, be patient!)
- Script or automation hitting the API
- Network issue causing retries

**Fix**: Wait 30 seconds and try again. If persistent, contact admin.

### The page is stuck loading

**Fix**:
1. Check **footer status** (green dot = connected)
2. Refresh the page (Ctrl+R or Cmd+R)
3. Clear browser cache (Ctrl+Shift+R or Cmd+Shift+R)
4. Try a different browser
5. Contact admin if still stuck

### Transactions are appearing twice

**Likely causes**:
- Clicked "Log Hours" button twice (first click worked!)
- Network issue caused retry

**Fix**:
- If truly duplicated, contact treasurer to reverse one
- Prevention: Click once and wait for confirmation

### The UI looks broken on mobile

**Try**:
1. Rotate to landscape mode
2. Zoom out (pinch on screen)
3. Update browser to latest version
4. Try a different browser

If still broken, report to admin (may be a bug).

### How do I report a bug?

1. Note what you were doing when the error occurred
2. Check browser console for errors (F12 → Console tab)
3. Take a screenshot if helpful
4. Email your administrator with:
   - What happened
   - What you expected
   - Steps to reproduce
   - Browser and version

---

## Philosophy & Principles

### Why is everyone's time valued equally?

This is a core principle of timebanking (though not universally followed):

**Reasons**:
1. **Equality**: All humans have equal value
2. **Inclusivity**: Prevents class distinctions
3. **Reciprocity**: Encourages diverse exchanges
4. **Simplicity**: No need to negotiate "exchange rates"

Some timebanks use **weighted hours** (e.g., licensed professionals = 1.5x). Your cooperative decides its own principles.

### What's the difference between a timebank and money?

| Timebank | Money |
|----------|-------|
| Time is the unit | Dollars/euros/etc. |
| Equal value per hour | Market-determined prices |
| Can't invest or lend at interest | Can earn interest |
| Non-extractive | Can be extracted from community |
| Local circulation | Global circulation |
| Relationship-based | Transaction-based |

**Key difference**: Timebanks prioritize community relationships over profit.

### Can I earn a living from the timebank?

Probably not. Timebanks are best for:
- Supplemental exchanges
- Building community resilience
- Accessing services you couldn't afford
- Using skills in new ways

They're **not a replacement** for paid employment (yet). Think of it as a complement to the regular economy.

### What prevents cheating?

Several mechanisms:
1. **Transparency**: All transactions are public (within the cooperative)
2. **Reputation**: Repeated cheating damages trust
3. **Credit limits**: Prevent accumulating huge debts
4. **Governance**: Community can vote to suspend members
5. **Small communities**: Harder to cheat people you know

**Perfect system?** No. **Better than nothing?** Yes.

### What if demand doesn't match supply?

Common scenario: More people want childcare than can provide it.

**Solutions**:
1. **Recruit** new members with needed skills
2. **Train** existing members (offer workshops)
3. **Adjust norms** (e.g., family members can help)
4. **Expand definition** (e.g., "elder care" ≈ "childcare")
5. **Accept limitations** (timebanks can't solve everything)

### Why use blockchain/P2P instead of a central database?

ICN is **not a blockchain** (no mining, no consensus algorithm). It's **peer-to-peer** (P2P):

**Benefits**:
- No central authority to shut down
- Cooperatives own their data
- Resilient to server failures
- Aligns with cooperative values (decentralization)

**Tradeoffs**:
- More complex to set up
- Requires some technical knowledge
- Fewer out-of-the-box tools

For small pilots, a central database might be simpler. ICN is designed for growth and federation.

### Can cooperatives federate (connect with each other)?

**Future feature!** The vision:
- Multiple cooperatives run separate ICN nodes
- Members can exchange across cooperatives
- Trust graph determines inter-coop credit limits
- Governance remains local

**Current state**: Single cooperative per deployment. Federation requires additional development.

### How is this different from cryptocurrency?

| ICN Timebank | Cryptocurrency |
|--------------|----------------|
| Time is unit | Tokens/coins |
| Cannot speculate | Can speculate (trade for profit) |
| Local community | Global market |
| Democratic governance | Varies (often plutocracy) |
| Low energy use | High energy use (PoW) |
| Mutual aid focus | Investment focus |

**Some similarities**: Cryptography, decentralization, digital ledger.

**Philosophy**: ICN is for **mutual aid**, crypto is often for **speculation**.

### Is this legal?

**Generally yes**, but:
- Timebanks are legal in most countries (as barter)
- Check your local laws (some jurisdictions have restrictions)
- Tax implications vary (usually no tax on barter if under certain amounts)
- Consult a lawyer if offering professional services

**ICN is software**—the legal questions are about timebanking, not the technology.

---

## Still Have Questions?

### Documentation

- [Quick Start Guide](QUICK-START.md) - Getting started (5 minutes)
- [Treasurer's Guide](TREASURER-GUIDE.md) - Financial management
- [Admin Guide](ADMIN-GUIDE.md) - System administration

### Support

- **Your cooperative**: Contact your admin or treasurer
- **Technical issues**: Email your administrator
- **ICN project**: https://github.com/InterCooperative-Network/icn/issues

### Community

Join discussions with other ICN cooperatives (if available) to share experiences and best practices.

---

**Remember**: Timebanking is about relationships, not just transactions. Be patient, be generous, and help your community thrive! 🌱
