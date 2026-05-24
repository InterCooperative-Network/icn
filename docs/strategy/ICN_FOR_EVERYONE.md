---
Status: descriptive
Canonical: no
Last Reviewed: 2026-05-21
---

# ICN, in Plain English

> A friendly introduction for anyone who's heard about ICN and asked "wait, what is that?"
>
> No tech background required. No politics knowledge required. We'll start from "what's a cooperative" and work outward.

---

## Start here

Most of the software you use is owned by companies that make money by:

1. Charging you a subscription.
2. Showing you ads.
3. Selling your data.
4. Locking you in so you can't leave easily.
5. Getting bought by a bigger company that does more of all four.

You probably know this. You've felt it. The bank app that gets worse every year. The social network that filled with ads. The note-taking app that raised prices once it had your data hostage.

There's a different way to run an organization — **cooperatives** — and there's a problem they have, and ICN is one attempt to help. This document explains all three.

---

## What's a cooperative?

A cooperative is an organization **owned and democratically controlled by the people who use it or work in it**, not by outside investors.

You almost certainly belong to several already, even if nobody told you.

| If you... | You belong to a cooperative |
|---|---|
| Bank at a credit union | Member-owned financial cooperative |
| Shop at REI | Consumer cooperative (~24 million members) |
| Live in rural America with electricity | Probably served by a rural electric cooperative |
| Have insurance from State Farm, Nationwide, or USAA | Mutual insurance company (cooperative cousin) |
| Buy from ACE Hardware | Retailer-owned cooperative |
| Drink Sunkist orange juice or Ocean Spray cranberry juice or eat Land O'Lakes butter | Agricultural cooperative |
| Shop at a local food co-op | Consumer cooperative |
| Work at a worker-owned business (King Arthur Flour, New Belgium Brewing, Equal Exchange) | Worker cooperative |

There are roughly **3 million cooperatives worldwide** with **1 billion members**. The cooperative sector employs about 280 million people globally — more than all multinational corporations combined. 2025 was the **UN International Year of Cooperatives** because the model matters.

Cooperatives can be:
- **Consumer cooperatives** (you own the store you shop at — REI, food co-ops, credit unions)
- **Worker cooperatives** (the workers own and run the business — Equal Exchange, Cooperative Home Care Associates)
- **Producer cooperatives** (farmers own the processor and brand — Sunkist, Ocean Spray)
- **Housing cooperatives** (residents collectively own the building)
- **Multi-stakeholder cooperatives** (combining workers, consumers, producers, community)
- **Federations of cooperatives** (cooperatives of cooperatives — the Mondragon Corporation in Spain has 80,000 worker-owners across 100+ companies)

The basic deal: **the people most affected by an organization own it and make decisions about it democratically.** One member, one vote — not one dollar, one vote.

---

## Why do cooperatives matter?

Three reasons that tend to land:

### 1. Who gets the money
In a regular company, profits go to shareholders. In a cooperative, surplus is distributed to members (often as "patronage refunds") or reinvested in things members value. The same dollar of profit either flows to a stranger holding stock or stays with the people who created the value.

### 2. Who makes the decisions
In a regular company, big decisions are made by executives accountable to shareholders. In a cooperative, big decisions are made by members or by a board the members elect. When your credit union changes its overdraft policy, it's a decision your member-elected board made. When your bank does, it's a decision a CEO made to please Wall Street.

### 3. Who's there in the long run
Companies get acquired, go public, change strategy, leave town. Cooperatives, on average, last longer and are more rooted in their community. Rural electric cooperatives served farms for 90 years that no private company would serve. Local credit unions served neighborhoods that big banks redlined.

This isn't romance. It's structural. **Who owns a thing changes what the thing does.**

---

## So what's the problem?

Cooperatives exist. Have for 180 years. They work. The model is durable.

The problem is **the software cooperatives use to run themselves doesn't share their values.**

Think about what a cooperative needs to do:
- Keep track of members (who's a member? since when? what kind?)
- Hold democratic votes (what did we decide? when? was the threshold met?)
- Track patronage (what did each member contribute? what's their share of surplus?)
- Manage governance documents (what are the current bylaws? what did the board approve last year?)
- Coordinate with other cooperatives (joint purchasing, mutual aid, federation membership)
- Survive turnover (when founders leave, do the records survive?)

Today, all of that work happens in software made by companies that have nothing to do with cooperatives:

- Member records → Mailchimp or Google Sheets (companies)
- Voting → Loomio (a B-Corp, but still external) or SurveyMonkey (a company)
- Documents → Google Workspace or Dropbox (companies)
- Patronage tracking → QuickBooks (a company) plus spreadsheets
- Communications → Slack, Gmail (companies)
- Coordination with other coops → email threads (no system at all)

When the vendor raises prices, the cooperative absorbs the cost. When the vendor changes its terms, the cooperative complies or migrates. When the vendor shuts down, the cooperative loses its records. When the founder who set up the Google Drive leaves, the cooperative may lose access to its own history.

This is a structural mismatch. A movement built on democratic ownership runs on infrastructure built for the opposite values.

---

## What ICN is, in plain English

ICN — the **InterCooperative Network** — is software that helps cooperatives:

1. **Keep durable records** of who's a member, what was decided, what was authorized, what's owed, and what actually happened.
2. **Verify each other** across organizational boundaries (so two coops can coordinate without trusting a vendor).
3. **Survive turnover** — when founders leave or vendors change, the institutional memory doesn't disappear.
4. **Coordinate as a movement** — so the cooperative economy can actually act as an ecosystem, not just a collection of unrelated organizations.

The crucial part: **ICN is not a vendor.** Each cooperative runs its own copy on its own server (or a small server hosted by a movement-aligned partner). There's no central platform, no subscription, no company that owns the data. The software is open source — anyone can read it, modify it, run it.

Long-term goal: ICN is **infrastructure the cooperative movement owns**, the way the road system is infrastructure the public owns. Not a service rented from a company.

---

## How does it work, briefly?

You don't need to know this to use a future cooperative running on ICN. But for the curious:

ICN organizes information around eight ideas that every democratic organization deals with:

- **Standing**: who has the right to act in this context (member, board, officer)
- **Authority**: who can authorize what (the board can engage a CPA; only members can change bylaws)
- **Decision**: a choice the cooperative made, under authority, with a threshold met
- **Obligation**: a commitment the decision created (a patronage allocation, an advisor agreement)
- **Effect**: the actual change in the world (member admitted, allocation lands, bylaw adopted)
- **Receipt**: a verifiable record that the decision and effect happened, signed in a way that proves it
- **Evidence**: the receipts plus the context (the bylaws, the prior decisions) someone needs to interpret them later
- **Review**: any member, board, federation, regulator can verify the chain without trusting a vendor

This is what's going on under the hood. Most members of a future cooperative running on ICN would just use the apps — they would never think about "standing" or "receipts" any more than you think about TCP/IP when you send an email.

The point is that the **records are real, signed, and verifiable**. If your housing cooperative made a decision in 2026 about your equity, a successor board in 2036 can verify exactly what was decided, by whom, under what authority, and what it caused — without trusting any specific vendor to still exist.

---

## What ICN is NOT

A short, honest list:

- **Not crypto.** Not a token. Not a coin. Not a blockchain in the speculative sense. The cryptography is invisible plumbing for signing records — the same way TLS makes your bank's website secure without you having to think about it. If anyone says "decentralized" and you think Web3, you're thinking of something else.
- **Not a startup.** No investors. No "growth at all costs." No exit. Funded by grants and volunteer time.
- **Not a vendor.** There's no subscription. No platform you log into. No company that owns your data. Each cooperative runs its own copy.
- **Not for sale.** ICN, long-term, is owned and governed by the cooperatives that use it. It is not anyone's product to sell.
- **Not replacing the human work of cooperation.** Software can hold records. Software cannot resolve conflict, build trust, or make a cooperative actually democratic. ICN holds the *evidence* that humans did the work; it doesn't do the work.
- **Not a bank, payment app, wallet, or accounting system.** Coops still need CPAs, banks, payment processors, accounting software. ICN sits beside those, holding the governance side of economic events (the decision to allocate; the authority that approved it), not the books themselves.
- **Not ready for production.** ICN is pre-pilot. The infrastructure runs in test environments. The apps a cooperative member would actually use are still being built. This document is about what it's *trying to be*, not what's ready to use today.

---

## A useful mental model

Imagine the cooperative movement is a city of small democratically-owned shops, restaurants, banks, farms, and housing collectives. The buildings work. The people are committed. The values are clear.

But the city's **roads** — the infrastructure that lets the shops talk to each other, deliver goods, settle accounts, coordinate emergencies, federate into something bigger — those roads are owned by a private company. The company charges tolls. The company changes the rules. The company sometimes closes roads it doesn't think are profitable. The company is for sale, and the next owner might be worse.

ICN is one attempt to **build a road system the city itself owns**.

Not the shops. The roads. The shops are fine. They just need infrastructure that doesn't have a profit motive at odds with the city's values.

---

## Why now?

Three reasons converge:

1. **The cooperative movement is growing.** Worker cooperatives in the US grew ~30% in the last decade. State and municipal governments are funding cooperative-development programs. The 2025 UN International Year of Cooperatives drew real attention.
2. **Vendor capture is getting worse.** The same companies that hosted the cooperative-tech stack ten years ago have been bought, raised prices, changed terms, or sold user data. The pattern is now obvious.
3. **The technology is ready.** Peer-to-peer infrastructure, cryptographic verification, content-addressed storage, encrypted transport — all production-grade now, used by other things you depend on. ICN isn't inventing new tech. It's assembling proven pieces for a specific institutional problem.

---

## What can you do about it?

If you're not in the cooperative tech world, the honest answer is mostly: **support cooperatives in your daily life and let the infrastructure work happen in the background.**

Practical things:

- **Bank at a credit union** instead of a big bank. If your credit union sucks, find a better one — they vary widely.
- **Shop at a food co-op** if your area has one. If it doesn't, find out why and whether a group is trying to start one.
- **Notice cooperatives in your life** and notice when a service you used to like got worse because a company bought it. The contrast is often instructive.
- **Ask companies you buy from whether they're employee-owned, worker-owned, or worker-controlled** (this category includes a surprising number of brands).
- **If you have a skill that could help a cooperative** — legal, accounting, design, software, organizing — consider donating some time to a coop developer or TA provider in your region. Cooperatives are chronically short on professional help.
- **Tell people what a cooperative is.** Most people have no idea their bank could be one, that their grocery store could be one, that their job could be one. The vocabulary itself is half the battle.

If you're in the tech world or you know people who are:
- **Don't build crypto/Web3 stuff and pitch it to cooperatives.** They've heard it. It's almost always a misfit.
- **Do consider funding or contributing to movement-aligned infrastructure projects** (ICN, Co-op Cloud, MayFirst, RPS, and others). These projects survive because people give them time and money.
- **Don't try to build "Uber for X" but cooperative.** The model isn't the same. Talk to actual cooperative developers before you build.

---

## FAQ

### "Wait — is this a crypto thing?"
No. ICN uses cryptographic signing the same way your bank's website does — to prove records are real and haven't been tampered with. No tokens. No coins. No speculation. If you've heard of Loomio, Open Collective, Mastodon, or Mattermost — ICN is in that family of "alternatives to vendor SaaS" software, not the crypto family.

### "Is this socialist?"
Cooperatives have existed across every political tradition for 180 years. Conservative rural farmers built the rural electric cooperatives. Catholic worker movements built Mondragon. Suburban families belong to REI. Urban progressives shop at food co-ops. Free-market economists like Elinor Ostrom won the Nobel Prize for studying cooperative commons governance. The model is older than the modern left-right axis and crosses it. If anyone tells you cooperatives are a political team flag, they're mostly wrong.

### "How does this make money?"
It doesn't, on purpose. ICN is funded by grants and volunteer time, like a public library or a road. Long-term, the cooperatives that use it pay for its maintenance the way they pay for shared services they use, not the way you pay for Netflix.

### "What if a bad actor uses it?"
The same risk every piece of open-source infrastructure carries. The safeguards are in how cooperatives use it (governance, accountability, federation), not in trying to lock down who can run the software. We assume bad actors will try; we design for verifiability rather than for gatekeeping.

### "What's the catch?"
Three catches, honestly:
1. **It's pre-pilot.** Cool to talk about; not ready to use today.
2. **It only matters if cooperatives actually use it**, which means it has to be genuinely useful to them, which is a much harder bar than "we built something cool."
3. **The hard problems aren't technical.** They're institutional. Cooperative federation has been a hard problem for 100+ years for political and organizational reasons, not technical ones. ICN is one piece of the puzzle, not the whole solution.

### "How is this different from a Mastodon instance for cooperatives?"
Mastodon is federated social media — good for messaging across servers. ICN is federated *governance and records* — designed for the institutional work that has to survive turnover and verify across organizational boundaries. Different problem, different shape.

### "Who's behind it?"
Currently a small group of people doing this work for love and grant money. The intention is for ICN's governance to eventually pass to a federation of the cooperatives that use it. Not a company. Not a foundation owned by tech billionaires.

### "What if it fails?"
The software is open source. If the people doing this work step away tomorrow, anyone can fork it. The substrate that runs runs. The pieces that don't work yet don't disappear; they just stay unfinished until someone picks them up. That's the resilience of open infrastructure as opposed to a startup.

### "What can I read next if I'm curious?"
Three good starts:
- **About cooperatives**: the US Federation of Worker Cooperatives (usworker.coop), the International Cooperative Alliance (ica.coop), or any of the books on the cooperative movement by Jessica Gordon Nembhard or John Restakis.
- **About cooperative-friendly tech**: Co-op Cloud (coopcloud.tech), MayFirst (mayfirst.coop), CommonsHosting.
- **About ICN specifically**: the source code at github.com/InterCooperative-Network/icn, or the longer document for cooperative-movement folks at `docs/strategy/ICN_FOR_COOPERATIVE_MOVEMENT.md`.

---

## In a sentence

The cooperative movement is real and works. The software it runs on doesn't share its values. ICN is one attempt to build the missing layer — software that cooperatives own, that helps them coordinate as an ecosystem, that doesn't sell anyone out.

It's not done yet. It might fail. It's worth trying.

---

*Pre-pilot · Open source · Not a vendor · Not for sale · Source: github.com/InterCooperative-Network/icn*
