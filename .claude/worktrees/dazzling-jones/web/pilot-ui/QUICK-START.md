# New User Quick Start Guide

Welcome to your cooperative's timebank! This guide will get you up and running in 5 minutes.

## What is a Timebank?

A timebank is a way for community members to exchange services using time as currency. When you help someone for 1 hour, you earn 1 time credit. You can then spend that credit to receive help from anyone else in the community.

**Key Principle**: Everyone's time is valued equally. An hour is an hour, regardless of the service provided.

## Getting Started

### Step 1: Access the Timebank

1. Open your web browser (Chrome, Firefox, Safari, or Edge)
2. Navigate to your cooperative's timebank URL (provided by your admin)
3. You'll see the login screen

### Step 2: Get Your Authentication Token

Don't have a token yet? Click the **"How do I get a token?"** button on the login screen.

1. **Open your terminal** (command prompt on Windows, terminal on Mac/Linux)
2. **Run the command** shown in the popup:
   ```bash
   icnctl auth login --gateway http://localhost:8080 --coop your-coop-id
   ```
   Replace `your-coop-id` with your cooperative's ID (ask your admin if unsure)
3. **Copy the token** that appears in your terminal
4. **Paste it** into the "Authentication Token" field

### Step 3: Sign In

1. Enter the **Gateway URL** (usually `http://localhost:8080` or provided by your admin)
2. Enter your **Cooperative ID** (e.g., `garden-coop`, `tool-library`)
3. Enter your **DID** (your unique identifier - run `icnctl id show` if you don't know it)
4. Paste your **authentication token**
5. Click **"Connect"**

🎉 **You're in!** Your token is valid for 24 hours. A countdown timer in the header shows when it expires.

## Your Dashboard

After signing in, you'll see:

- **My Balance**: Your current time balance
  - **Green (positive)**: You have credit - others owe you time
  - **Red (negative)**: You owe time to others
  - **Arrow indicator**: Shows if your balance is trending up ↑, down ↓, or stable →
- **Members**: Total number of members in your cooperative
- **Hours This Month**: Total hours exchanged in the last 30 days
- **Recent Activity**: Last 5 transactions in the community

## How to Log Service Hours

When you help another member, log it so you receive credit:

1. Click the **"Log Hours"** tab
2. Select **who you helped** from the dropdown
3. Enter **how many hours** (e.g., 2.5 for 2 hours 30 minutes)
4. Add a **description** of the service (e.g., "Garden weeding")
5. Click **"Log Hours"**

✅ **Done!** You'll see a confirmation, and your balance will update.

**Important**: When YOU provide a service, YOUR balance increases and THEIR balance decreases (they now owe you).

## Viewing Your Transaction History

Want to see all your exchanges?

1. Click the **"History"** tab
2. Use the **time filter** to narrow results:
   - Today
   - This Week
   - **This Month** (default)
   - This Year
   - All Time
3. Use the **sort dropdown** to reorder:
   - Newest First (default)
   - Oldest First
   - Highest Amount
   - Lowest Amount
4. Click **"Export CSV"** to download for Excel/Google Sheets

## Finding Members

Need to find someone?

1. Click the **"Members"** tab
2. Type in the **search box** to filter by DID
3. Click the **📋 button** next to any DID to copy it to clipboard

## Voting on Proposals

Your cooperative makes decisions together:

1. Click the **"Governance"** tab
2. Review **active proposals**
3. Vote **For**, **Against**, or **Abstain** on each
4. Closed proposals show the **outcome** (Accepted/Rejected)

**Tip**: Red countdown = urgent! Vote before it closes.

## Keyboard Shortcuts

Speed up your navigation:

- **Ctrl+1** (or Cmd+1 on Mac): Dashboard
- **Ctrl+2**: Log Hours
- **Ctrl+3**: History
- **Ctrl+4**: Members
- **Ctrl+5**: Governance

## Understanding Your Balance

### Positive Balance (Green)
You've provided more hours than you've received. Other members owe you time. You can "spend" these credits by requesting help.

### Negative Balance (Red)
You've received more hours than you've provided. You owe time to the community. Provide services to bring your balance up.

### Zero Balance
You've exchanged equally. This is common for active members who give and receive regularly.

**Note**: A negative balance isn't bad! It means you're receiving help from your community. The goal is to keep balances circulating, not to hoard credits.

## Token Expiration

Your authentication token expires after **24 hours** for security.

**Watch for these indicators**:
- **Green badge** (>1 hour left): All good
- **Yellow badge** (<1 hour left): Token expiring soon
- **Red badge** (<15 minutes left): Get a new token!

You'll receive automatic warnings at 15, 10, and 5 minutes before expiration.

**To refresh**: Click "Logout" → Get a new token → Sign in again

## Real-Time Updates

The app updates automatically every 30 seconds. You'll also see:

- **Toast notifications** (top-right corner) for actions
- **WebSocket status** (footer) - green dot = connected
- **Last update time** (footer)

If the connection drops (red dot), the app will reconnect automatically.

## Getting Help

### Common Issues

**"Cannot connect to the server"**
- Check that the gateway is running
- Verify the gateway URL is correct
- Ask your admin if the service is online

**"Your session has expired"**
- Your token expired (24-hour limit)
- Sign out and get a new token

**"You don't have permission"**
- Your token doesn't have the required permissions
- Contact your cooperative administrator

**"No members showing"**
- Verify your cooperative ID is correct
- Check that members have been added
- Refresh the page

### Need More Help?

- Check the **Treasurer's Guide** for managing finances
- Check the **Admin Guide** for administration tasks
- Check the **FAQ** for common questions
- Contact your cooperative administrator

## Tips for Success

1. **Log services promptly** - Don't wait days to record exchanges
2. **Be descriptive** - Add clear memos so everyone knows what was exchanged
3. **Check your balance weekly** - Stay aware of your giving/receiving ratio
4. **Vote on proposals** - Participate in community decisions
5. **Be generous** - Remember, everyone's time is valued equally
6. **Ask for help** - That's what the timebank is for!

## Next Steps

Now that you're set up:

1. **Browse the member list** - See who's in your cooperative
2. **Log your first service** - Start building credit
3. **Review the history** - See what others are exchanging
4. **Vote on a proposal** - Make your voice heard

Welcome to the community! 🎉

---

**Questions?** Contact your cooperative administrator or check the FAQ.

**Technical Issues?** Report them at https://github.com/InterCooperative-Network/icn/issues
