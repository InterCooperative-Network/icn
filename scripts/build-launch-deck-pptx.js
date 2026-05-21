#!/usr/bin/env node
/**
 * Build the Launch + ICN screen deck as a .pptx.
 *
 * Output: docs/strategy/COOPERATIVE_FORMATION_PLATFORM_SCREEN_DECK_2026-05-21.pptx
 *
 * Design intent: calm, minimal, light background, dark text. No decorative
 * imagery. One visual motif: a thin left-side accent rule on every slide.
 * Per-slide layout variation only where it earns its keep (title cover, pull
 * quote, horizontal spine, muted boundary card, large closing question).
 *
 * Speaker notes are written as spoken copy — what Matt would actually say,
 * with brief [stage directions] inline.
 */

const path = require("path");

function resolvePptxgen() {
  try {
    return require("pptxgenjs");
  } catch (_) {
    // NODE_PATH may contain multiple directories separated by path.delimiter
    // (":" on POSIX, ";" on Windows); split before probing.
    const nodePathDirs = (process.env.NODE_PATH || "")
      .split(path.delimiter)
      .filter(Boolean);
    const appdataDir =
      process.env.APPDATA && path.join(process.env.APPDATA, "npm", "node_modules");
    const candidates = [
      ...nodePathDirs,
      appdataDir,
      "/usr/local/lib/node_modules",
      "/usr/lib/node_modules",
    ].filter(Boolean);
    for (const dir of candidates) {
      try {
        return require(path.join(dir, "pptxgenjs"));
      } catch (_) {}
    }
    throw new Error("pptxgenjs not found. Install with: npm install -g pptxgenjs");
  }
}
const pptxgen = resolvePptxgen();

const OUT = path.resolve(
  __dirname,
  "..",
  "docs",
  "strategy",
  "COOPERATIVE_FORMATION_PLATFORM_SCREEN_DECK_2026-05-21.pptx",
);

// --- Palette ---------------------------------------------------------------
const INK = "1A1D21"; // primary text
const INK_SOFT = "4A4F55"; // muted text
const INK_FAINT = "8A8F95"; // chip / footer
const RULE = "2B5876"; // accent rule, accent text
const RULE_SOFT = "C5D2DC"; // accent rule on muted slides
const PAPER = "FCFCFA"; // primary background
const PAPER_WARM = "F7F3EA"; // muted / personal slide background
const QUOTE_BG = "EAF1F6"; // accent fill for the pull-quote
const QUOTE_BORDER = "B6CBDA"; // border for pull-quote
const WARN_BG = "F4ECE7"; // boundary slide background
const WARN_INK = "8C5544"; // boundary slide accent
const CHIP_BG = "FFFFFF";
const CHIP_BORDER = "1A1D21";

// --- Layout ----------------------------------------------------------------
const SLIDE_W = 13.333; // widescreen
const SLIDE_H = 7.5;
const MARGIN_LEFT = 0.9;
const MARGIN_RIGHT = 0.9;
const MARGIN_TOP = 0.7;
const CONTENT_W = SLIDE_W - MARGIN_LEFT - MARGIN_RIGHT;

const HEADER_FONT = "Calibri";
const BODY_FONT = "Calibri";

// --- Master chrome ---------------------------------------------------------
function addChrome(slide, slideNum, totalSlides, opts = {}) {
  slide.background = { color: opts.bg || PAPER };
  // Left accent rule
  slide.addShape("rect", {
    x: 0,
    y: 0,
    w: 0.18,
    h: SLIDE_H,
    fill: { color: opts.ruleColor || RULE },
    line: { color: opts.ruleColor || RULE, width: 0 },
  });
  // Slide number chip
  if (!opts.suppressNumber) {
    slide.addText(`${slideNum} / ${totalSlides}`, {
      x: SLIDE_W - 1.6,
      y: 0.32,
      w: 1.3,
      h: 0.3,
      fontFace: BODY_FONT,
      fontSize: 10,
      color: INK_FAINT,
      align: "right",
      charSpacing: 4,
    });
  }
  // Footer
  if (!opts.suppressFooter) {
    slide.addText(opts.footer || "Launch + ICN · 2026-05-21 · reciprocal conversation", {
      x: MARGIN_LEFT,
      y: SLIDE_H - 0.46,
      w: CONTENT_W,
      h: 0.3,
      fontFace: BODY_FONT,
      fontSize: 9,
      color: INK_FAINT,
      align: "left",
    });
  }
}

function addTitle(slide, text, opts = {}) {
  slide.addText(text, {
    x: MARGIN_LEFT,
    y: opts.y ?? MARGIN_TOP,
    w: CONTENT_W,
    h: opts.h ?? 1.0,
    fontFace: HEADER_FONT,
    fontSize: opts.size ?? 36,
    bold: opts.bold !== false,
    color: opts.color || INK,
    align: "left",
    valign: "top",
  });
}

function addBullets(slide, items, opts = {}) {
  const lines = items.map((t, i) => ({
    text: t,
    options: {
      bullet: { code: "2022" },
      breakLine: i < items.length - 1,
      paraSpaceAfter: opts.spaceAfter ?? 10,
    },
  }));
  slide.addText(lines, {
    x: MARGIN_LEFT + 0.1,
    y: opts.y ?? 1.95,
    w: CONTENT_W - 0.1,
    h: opts.h ?? 4.7,
    fontFace: BODY_FONT,
    fontSize: opts.size ?? 24,
    color: opts.color || INK,
    align: "left",
    valign: "top",
    lineSpacingMultiple: 1.25,
  });
}

// --- Build ----------------------------------------------------------------
const pres = new pptxgen();
pres.title = "Launch + ICN: From Formation to Continuity";
pres.author = "Matt Faherty";
pres.subject = "2026-05-21 reciprocal conversation deck";
pres.layout = "LAYOUT_WIDE";

const TOTAL = 11;

// ----- Slide 1: TITLE COVER -----
{
  const s = pres.addSlide();
  addChrome(s, 1, TOTAL, { suppressNumber: true, suppressFooter: true });

  // Eyebrow label
  s.addText("LAUNCH + ICN · 2026-05-21", {
    x: MARGIN_LEFT,
    y: 2.4,
    w: CONTENT_W,
    h: 0.4,
    fontFace: BODY_FONT,
    fontSize: 13,
    color: RULE,
    bold: true,
    charSpacing: 8,
  });

  // Big title
  s.addText("From Formation to Continuity", {
    x: MARGIN_LEFT,
    y: 2.85,
    w: CONTENT_W,
    h: 1.5,
    fontFace: HEADER_FONT,
    fontSize: 60,
    bold: true,
    color: INK,
    align: "left",
    charSpacing: -2,
  });

  // Subtitle
  s.addText(
    "A conversation about what cooperatives need after they begin, and the ecosystem that lets them materially exist.",
    {
      x: MARGIN_LEFT,
      y: 4.45,
      w: CONTENT_W - 1.0,
      h: 1.0,
      fontFace: HEADER_FONT,
      fontSize: 22,
      color: INK_SOFT,
      italic: false,
      lineSpacingMultiple: 1.3,
    },
  );

  // Meta strip
  s.addText("10 minutes · screen-share · reciprocal · for Launch first, ICN second", {
    x: MARGIN_LEFT,
    y: SLIDE_H - 0.9,
    w: CONTENT_W,
    h: 0.4,
    fontFace: BODY_FONT,
    fontSize: 12,
    color: INK_FAINT,
    charSpacing: 4,
  });

  s.addNotes(
    [
      "OPEN (≈45 sec). Spoken copy:",
      "",
      "“Thanks for making time, McKenzie. Before I share anything else, two quick things.",
      "First, who else is on the call with us today?”",
      "[Wait. Let her introduce whoever is curious.]",
      "",
      "“Second, I want to be honest about the shape of this conversation. I want to learn how Launch thinks about the formation workflow, and the bigger ecosystem you and the Worker Place and comp.coop are working toward. I'll share enough of what ICN is trying to be that you can tell me where the map is wrong. But the most useful thing for me today is your map. Sound good?”",
      "",
      "[Wait for her green light. Then advance.]",
    ].join("\n"),
  );
}

// ----- Slide 2: What Launch is solving -----
{
  const s = pres.addSlide();
  addChrome(s, 2, TOTAL);
  addTitle(s, "What I understand Launch is solving");
  addBullets(s, [
    "Starting a worker co-op is too hard.",
    "Resources are scattered.",
    "State rules vary.",
    "TA capacity is limited.",
    "Groups need the right help at the right time.",
  ]);
  s.addNotes(
    [
      "PURPOSE: show I read Launch's public material, then ask Launch to correct me. ≈40 sec.",
      "",
      "Spoken copy:",
      "",
      "“Here's the read I have from Launch's public material. [Walk the list briefly.] Starting a worker co-op is too hard. Resources are scattered. State rules vary. TA capacity is limited. Groups need the right help at the right time.”",
      "",
      "[Beat.]",
      "",
      "“Does that read feel right from inside Launch, or am I missing something fundamental about the problem you're solving?”",
      "",
      "[Listen. Take notes verbatim. If she corrects the framing, USE her language for the rest of the call.]",
    ].join("\n"),
  );
}

// ----- Slide 3: Launch's public shape -----
{
  const s = pres.addSlide();
  addChrome(s, 3, TOTAL);
  addTitle(s, "Launch's public shape");
  addBullets(s, [
    "Onboarding questions, decision-tree style.",
    "Teams, tasks, comments, documents.",
    "Advisors and service providers.",
    "Secure document portal.",
    "Mobile, print, multilingual access.",
  ]);
  s.addNotes(
    [
      "PURPOSE: signal I did the surface read, then transition fast to the listening slide. ≈25 sec. Do not dwell.",
      "",
      "Spoken copy:",
      "",
      "“And here's the surface read. [Quick pass.] Onboarding questions in a decision-tree style. Teams, tasks, comments, documents. Advisors and service providers. Secure document portal. Mobile, print, multilingual.”",
      "",
      "“That's the outside view. I want to know what the inside view looks like — what states a group actually moves through. So let me ask it as five questions on the next slide.”",
    ].join("\n"),
  );
}

// ----- Slide 4: Listen slide -----
{
  const s = pres.addSlide();
  addChrome(s, 4, TOTAL);
  addTitle(s, "What I want to learn from you");
  addBullets(s, [
    "Where does the workflow begin and end?",
    "What states does a group move through?",
    "What do advisors need to see?",
    "What records matter later?",
    "What should software never touch?",
  ]);
  s.addNotes(
    [
      "THE MOST IMPORTANT SLIDE OF THE MEETING. Listen, don't lecture.",
      "Aim for 3 to 4 minutes here. Do not advance to slide 5 until she's done.",
      "",
      "Spoken copy:",
      "",
      "“These are the five things I most want to understand from your side. [Read each, slowly:] Where does the workflow begin and end? What states does a group move through? What do advisors need to see? What records matter later? What should software never touch?”",
      "",
      "“Take any of them. Start anywhere. I'm here to listen, not to teach Launch back to you.”",
      "",
      "[Now actually listen. Capture her vocabulary verbatim in your notes. If she asks me a clarifying question, answer briefly and hand it back. Do not narrate ICN on this slide.]",
      "",
      "If the conversation starts going long here, that is GOOD. Let it.",
    ].join("\n"),
  );
}

// ----- Slide 5: The seam (PULL QUOTE) -----
{
  const s = pres.addSlide();
  addChrome(s, 5, TOTAL);
  addTitle(s, "The seam I'm exploring", { y: MARGIN_TOP, h: 0.8 });

  // Pull-quote treatment
  const qx = MARGIN_LEFT;
  const qy = 1.85;
  const qw = CONTENT_W;
  const qh = 4.6;

  s.addShape("rect", {
    x: qx,
    y: qy,
    w: qw,
    h: qh,
    fill: { color: QUOTE_BG },
    line: { color: QUOTE_BORDER, width: 0.75 },
  });

  // Large opening quotation mark glyph
  s.addText("“", {
    x: qx + 0.15,
    y: qy - 0.05,
    w: 1.0,
    h: 1.3,
    fontFace: "Georgia",
    fontSize: 120,
    color: RULE,
    bold: true,
    valign: "top",
  });

  // First two sentences (regular)
  s.addText(
    "Launch helps cooperatives come into being. ICN is the question of what helps cooperatives continue governing, remembering, coordinating, and federating after formation.",
    {
      x: qx + 0.55,
      y: qy + 0.55,
      w: qw - 1.1,
      h: 2.1,
      fontFace: HEADER_FONT,
      fontSize: 24,
      color: INK,
      align: "left",
      valign: "top",
      italic: false,
      lineSpacingMultiple: 1.4,
    },
  );

  // The anchor sentence (italic, slightly bigger)
  s.addText(
    "The point is for a cooperative and solidarity-economy ecosystem to materially exist, not just ideologically exist.",
    {
      x: qx + 0.55,
      y: qy + 2.75,
      w: qw - 1.1,
      h: 1.5,
      fontFace: HEADER_FONT,
      fontSize: 26,
      color: RULE,
      align: "left",
      valign: "top",
      italic: true,
      bold: true,
      lineSpacingMultiple: 1.35,
    },
  );

  s.addNotes(
    [
      "THIS IS THE HEART OF THE DECK. ≈90 seconds. Then pause and hand the call back.",
      "",
      "Spoken copy:",
      "",
      "“Okay, so let me share where ICN is trying to sit. Just the framing, not a sales pitch.”",
      "",
      "[Read the quote aloud, slowly, with a beat between sentences:]",
      "",
      "“Launch helps cooperatives come into being. ICN is the question of what helps cooperatives continue governing, remembering, coordinating, and federating after formation. The point is for a cooperative and solidarity-economy ecosystem to materially exist, not just ideologically exist.”",
      "",
      "[Beat. Then say this once, citing the Summit material, not ICN's vocabulary:]",
      "",
      "“I want to be careful with that word ecosystem, because it's not my framing. The 2025 Summit ran an ecosystem-mapping session as one of its top-rated tracks. The kickoff vision named upstate-downstate bridging, cross-sector parity across worker, consumer, financial, ag, and housing co-ops, regional events between summits, and an NY state employee ownership center. The speaker series that went out under our committee's own name called the next wave ‘the infrastructure builders designing the legal, technological, and organizational backbone that lets co-ops scale.' That's the layer ICN is trying to be.”",
      "",
      "“Does that seam feel real from where you sit?”",
      "",
      "[Pause. Do not fill the silence. Let her answer first.]",
    ].join("\n"),
  );
}

// ----- Slide 6: What the ecosystem needs to remember -----
{
  const s = pres.addSlide();
  addChrome(s, 6, TOTAL);
  addTitle(s, "What the ecosystem needs to remember");
  addBullets(
    s,
    [
      "Member standing.",
      "Decisions and approvals.",
      "Governance documents.",
      "Advisor and service-provider handoffs.",
      "Patronage and capital-account history.",
      "Inter-cooperative trade, obligations, and federation evidence.",
      "Records future members, boards, CPAs, counsel, or a partner cooperative may need.",
    ],
    { size: 20, spaceAfter: 6, h: 5.0 },
  );
  s.addNotes(
    [
      "PURPOSE: ground the abstract seam in concrete records. ≈60 sec.",
      "",
      "Spoken copy:",
      "",
      "“If I'm right about the seam, here's the work that lives in it. [Walk the list briefly.] Member standing. Decisions and approvals. Governance documents. Advisor and service-provider handoffs. Patronage and capital-account history. Inter-cooperative trade, obligations, and federation evidence. And records future members, boards, CPAs, counsel, or a partner cooperative may need to verify later.”",
      "",
      "[One sentence on the ecosystem pattern, then a question.]",
      "",
      "“The pattern: each of these records belongs to a cooperative now, but might need to be verifiable to a partner co-op or federation later. That's the difference between an ecosystem that materially exists and one that's just shared values.”",
      "",
      "“Of that list, what jumps out as the thing your co-op work most needs and least has?”",
      "",
      "[Listen. The patronage-and-capital-account-history piece is the doorway she opened in her May 1 email. If she names it, slow down.]",
    ].join("\n"),
  );
}

// ----- Slide 7: ICN in one breath (HORIZONTAL SPINE) -----
{
  const s = pres.addSlide();
  addChrome(s, 7, TOTAL);
  addTitle(s, "ICN in one breath");

  // Subtitle
  s.addText("Seven words. Not in this order in real life, but this is the shape.", {
    x: MARGIN_LEFT,
    y: 1.75,
    w: CONTENT_W,
    h: 0.4,
    fontFace: BODY_FONT,
    fontSize: 16,
    color: INK_SOFT,
    italic: true,
  });

  const spine = [
    "Standing",
    "Authority",
    "Decision",
    "Obligation",
    "Receipt",
    "Evidence",
    "Review",
  ];
  // Build two rows so the chips don't get squeezed
  const row1 = spine.slice(0, 4);
  const row2 = spine.slice(4);

  function placeRow(row, y) {
    const gap = 0.25;
    const chipW = (CONTENT_W - gap * (row.length - 1)) / row.length;
    const chipH = 1.0;
    row.forEach((word, i) => {
      const x = MARGIN_LEFT + i * (chipW + gap);
      s.addShape("roundRect", {
        x,
        y,
        w: chipW,
        h: chipH,
        rectRadius: 0.08,
        fill: { color: CHIP_BG },
        line: { color: CHIP_BORDER, width: 1.25 },
      });
      s.addText(word + ".", {
        x,
        y,
        w: chipW,
        h: chipH,
        fontFace: HEADER_FONT,
        fontSize: 22,
        bold: true,
        color: INK,
        align: "center",
        valign: "middle",
      });
    });
  }
  placeRow(row1, 2.5);
  placeRow(row2, 3.85);

  // Caption underneath
  s.addText(
    "Not because every human process should become software. Because some records need to survive turnover without everyone reconstructing the institution from email, PDFs, and somebody's exhausted memory.",
    {
      x: MARGIN_LEFT,
      y: 5.4,
      w: CONTENT_W,
      h: 1.2,
      fontFace: HEADER_FONT,
      fontSize: 17,
      color: INK_SOFT,
      italic: true,
      lineSpacingMultiple: 1.35,
    },
  );

  s.addNotes(
    [
      "PURPOSE: compress ICN into seven words she can remember. ≈40 sec to recite.",
      "",
      "Spoken copy:",
      "",
      "“If I had to compress ICN into seven words, it'd be these. [Read across the rows, one breath if you can:] Standing. Authority. Decision. Obligation. Receipt. Evidence. Review.”",
      "",
      "[Beat. Then the why.]",
      "",
      "“Not because every human process should become software. Because some records need to survive turnover, without everyone reconstructing the institution from email, PDFs, and somebody's exhausted memory.”",
      "",
      "[Move on unless she asks. Do not pitch each word. If she asks about any one of them, use these:]",
      "",
      "STANDING — who has the right to act in this context.",
      "  How it works: signed records of admission, transition, exit. Cryptographic proof of membership at a moment in time.",
      "  Quick: not membership management. Membership EVIDENCE. So a new treasurer in 2030 can answer who actually voted on this in 2027.",
      "",
      "AUTHORITY — who can authorize what, under what governance rule.",
      "  How it works: expressed in CCL (Cooperative Contract Language). The kernel enforces the constraint without understanding the meaning.",
      "  Quick: always reducible to a rule + a role + a threshold. Structural, not personal.",
      "",
      "DECISION — a choice made under authority, with threshold met.",
      "  How it works: links the proposal that triggered it, the votes cast, the threshold reached, the effect authorized.",
      "  Quick: a vote is not a decision until the threshold is met AND the result is recorded. The recording IS the decision.",
      "",
      "OBLIGATION — a commitment created by a decision.",
      "  How it works: obligor, obligee, scope, triggering decision, evidence-of-completion.",
      "  Quick: replaces 'debt' / 'liability' in ICN vocabulary. Advisor agreements, patronage allocations, federation commitments all live here.",
      "",
      "RECEIPT — a verifiable record that an event happened.",
      "  How it works: signed, content-addressed, everything attached; verifiable independently of any platform.",
      "  Quick: receipts alone do not prove legitimacy. They prove the event. Authority and review are separate.",
      "",
      "EVIDENCE — receipts plus context, available later.",
      "  How it works: receipts plus the chain (charter, governance rules, prior decisions) needed to interpret them; survives platform migration.",
      "  Quick: the difference between 'we had a vote' and 'we can verify the vote across years and platforms.'",
      "",
      "REVIEW — anyone can verify the chain.",
      "  How it works: cryptographic check of signatures + inspection of the contextual chain to confirm authority was real.",
      "  Quick: without review, receipts are files. With review, they are evidence. This is what makes ICN useful as institutional memory.",
    ].join("\n"),
  );
}

// ----- Slide 8: What ICN is not (MUTED BOUNDARY) -----
{
  const s = pres.addSlide();
  addChrome(s, 8, TOTAL, { bg: PAPER, ruleColor: RULE_SOFT });

  // Boundary card
  const cx = MARGIN_LEFT;
  const cy = 1.5;
  const cw = CONTENT_W;
  const ch = 4.5;
  s.addShape("roundRect", {
    x: cx,
    y: cy,
    w: cw,
    h: ch,
    rectRadius: 0.12,
    fill: { color: WARN_BG },
    line: { color: WARN_INK, width: 1.0 },
  });

  // Eyebrow
  s.addText("BOUNDARY CHECK", {
    x: cx + 0.4,
    y: cy + 0.3,
    w: cw - 0.8,
    h: 0.35,
    fontFace: BODY_FONT,
    fontSize: 11,
    color: WARN_INK,
    bold: true,
    charSpacing: 8,
  });

  s.addText("What ICN is not", {
    x: cx + 0.4,
    y: cy + 0.7,
    w: cw - 0.8,
    h: 0.7,
    fontFace: HEADER_FONT,
    fontSize: 28,
    color: INK,
    bold: true,
  });

  const notList = [
    "Not replacing Launch.",
    "Not replacing TA providers, lawyers, CPAs, or co-op developers.",
    "Not accounting software.",
    "Not financial-intermediary software.",
    "Not production-ready or a pilot ask.",
  ];
  const notLines = notList.map((t, i) => ({
    text: t,
    options: {
      bullet: { code: "2022" },
      breakLine: i < notList.length - 1,
      paraSpaceAfter: 6,
    },
  }));
  s.addText(notLines, {
    x: cx + 0.6,
    y: cy + 1.6,
    w: cw - 1.2,
    h: ch - 1.8,
    fontFace: BODY_FONT,
    fontSize: 18,
    color: INK,
    lineSpacingMultiple: 1.25,
  });

  s.addNotes(
    [
      "PURPOSE: name the boundary once, quietly, then move past it. ≈30 sec.",
      "",
      "Spoken copy (short version, if she's not asking):",
      "",
      "“Quick boundary slide. ICN is not replacing Launch or any of the people who do the human work of forming co-ops. It's not accounting software. It's not a financial-intermediary product. It's not production-ready and this isn't a pilot ask.”",
      "",
      "[Move on.]",
      "",
      "If she asks directly about wallets, tokens, payments, or crypto, expand:",
      "",
      "“No wallet, no token, no payment rail, no speculative crypto frame. ICN uses cryptographic verification as record integrity, not as a casino. If anyone reads ‘decentralized' and thinks Web3, they're reading the wrong project.”",
      "",
      "[Then stop. Don't volunteer more.]",
    ].join("\n"),
  );
}

// ----- Slide 9: A useful test -----
{
  const s = pres.addSlide();
  addChrome(s, 9, TOTAL);
  addTitle(s, "A useful test, if anything here feels real");
  addBullets(s, [
    "Pick one fictional or sanitized workflow.",
    "Walk it from formation to governance.",
    "Ask what must survive.",
    "Ask who needs it later.",
    "Ask what should stay outside ICN.",
  ]);
  s.addNotes(
    [
      "PURPOSE: put a concrete next step on the table without asking for a yes. ≈35 sec.",
      "",
      "Spoken copy:",
      "",
      "“If anything I've said feels like it might be real, here's what a next step might look like. Not a pilot. Not a commitment.”",
      "",
      "[Walk the list:]",
      "",
      "“Pick one fictional or sanitized workflow. Walk it from formation to governance. Ask what must survive. Ask who needs it later. Ask what should stay outside ICN.”",
      "",
      "“It's one concrete pattern that either proves the seam or kills it cleanly. Either is useful.”",
      "",
      "[Don't push. The option is on the table. She'll either bite or not. If she's quiet, move to slide 10.]",
    ].join("\n"),
  );
}

// ----- Slide 10: The question for today (CLOSING) -----
{
  const s = pres.addSlide();
  addChrome(s, 10, TOTAL);

  // Eyebrow
  s.addText("THE QUESTION FOR TODAY", {
    x: MARGIN_LEFT,
    y: MARGIN_TOP + 0.1,
    w: CONTENT_W,
    h: 0.4,
    fontFace: BODY_FONT,
    fontSize: 13,
    color: RULE,
    bold: true,
    charSpacing: 8,
  });

  // Huge question text
  s.addText(
    "Where is this map useful, where is it wrong, and what should I learn from Launch before building anything around this seam?",
    {
      x: MARGIN_LEFT,
      y: 1.8,
      w: CONTENT_W,
      h: 4.5,
      fontFace: HEADER_FONT,
      fontSize: 38,
      color: INK,
      bold: false,
      italic: false,
      lineSpacingMultiple: 1.3,
    },
  );

  s.addNotes(
    [
      "CLOSING. ≈20 sec to read. Then SHUT UP.",
      "",
      "Spoken copy:",
      "",
      "“The question I came in with is on the screen. [Read it aloud, slowly:]",
      "",
      "‘Where is this map useful, where is it wrong, and what should I learn from Launch before building anything around this seam?'”",
      "",
      "[Then stop. Hand the call to her. The whole meeting comes down to what she says next.]",
      "",
      "If she answers with a concrete thing, capture it verbatim. If she answers with another question, follow her. If she says ‘I need to think,' that is also a real answer — propose a follow-up date and end the call cleanly.",
    ].join("\n"),
  );
}

// ----- Slide 11: Notes to capture (FOR MATT) -----
{
  const s = pres.addSlide();
  addChrome(s, 11, TOTAL, { bg: PAPER_WARM });

  // Eyebrow distinguishing it as a personal/private slide
  s.addText("FOR MATT · AFTER THE CALL", {
    x: MARGIN_LEFT,
    y: MARGIN_TOP + 0.1,
    w: CONTENT_W,
    h: 0.4,
    fontFace: BODY_FONT,
    fontSize: 13,
    color: WARN_INK,
    bold: true,
    charSpacing: 8,
  });

  addTitle(s, "Notes to capture", { y: 1.25, size: 32 });

  addBullets(
    s,
    [
      "Launch validated — what landed.",
      "Launch rejected — what she said is wrong, overbuilt, or unclear.",
      "New vocabulary — her exact words, verbatim.",
      "Privacy boundary — what she said should never be public or recorded.",
      "Possible tabletop — fictional or sanitized workflow worth rehearsing.",
      "No-go zones — what ICN should never touch in this domain.",
    ],
    { y: 2.3, size: 20, h: 4.5, spaceAfter: 8 },
  );

  s.addNotes(
    [
      "DO NOT DISPLAY THIS SLIDE TO MCKENZIE.",
      "",
      "If you're short on time, end at slide 10 and skip this slide. It's a debrief template for you, not part of the conversation.",
      "",
      "Fill these six buckets within 24 hours of the call, sanitized, and paste into the prep packet's §13 capture grid. Use her exact words for the vocabulary bucket. Do not paraphrase yet.",
    ].join("\n"),
  );
}

pres
  .writeFile({ fileName: OUT })
  .then((p) => console.log(`Wrote ${p}`))
  .catch((err) => {
    console.error(err);
    process.exit(1);
  });
