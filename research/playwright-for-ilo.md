# Playwright for ilo — Design Exploration

How should ilo programs automate browsers? Two approaches explored: wrapping Playwright as a tool server (practical) vs native CDP client (ambitious).

## The problem

An AI agent writing ilo code needs to automate browsers — navigate, click, fill forms, assert content, take screenshots. Playwright (Node.js) solves this for JavaScript. What's the ilo equivalent?

## Approach 1: Playwright tool server (recommended)

Wrap Playwright as an external process that ilo talks to via the tool provider interface (D1d). ilo programs declare browser actions as tools and call them like any other tool.

### Architecture

```
ilo program  →  ToolProvider (HTTP/stdio)  →  Playwright server (Node.js)  →  Browser (CDP)
```

The Playwright server is a thin Node.js process that:
1. Launches a browser on startup
2. Accepts commands as JSON-RPC over stdio or HTTP
3. Translates commands to Playwright API calls
4. Returns results as JSON

### Tool declarations

```
tool nav"Navigate to URL" url:t>R _ t timeout:30
tool click"Click element" sel:t>R _ t timeout:10
tool dclick"Double-click element" sel:t>R _ t timeout:10
tool fill"Fill input field" sel:t val:t>R _ t timeout:10
tool clear"Clear input field" sel:t>R _ t timeout:10
tool check"Check checkbox" sel:t>R _ t timeout:10
tool sel"Select option" sel:t val:t>R _ t timeout:10
tool txt"Get text content" sel:t>R t t timeout:5
tool attr"Get attribute" sel:t name:t>R t t timeout:5
tool html"Get inner HTML" sel:t>R t t timeout:5
tool vis"Check element visible" sel:t>R b t timeout:5
tool wait"Wait for selector" sel:t>R _ t timeout:30
tool shot"Screenshot to file" path:t>R t t timeout:10
tool eshot"Screenshot element" sel:t path:t>R t t timeout:10
tool eval"Evaluate JavaScript" code:t>R t t timeout:10
tool title"Get page title" >R t t timeout:5
tool url"Get current URL" >R t t timeout:5
```

### Example: login test in ilo

```
tool nav"Navigate" url:t>R _ t timeout:30
tool fill"Fill input" sel:t val:t>R _ t timeout:10
tool click"Click" sel:t>R _ t timeout:10
tool txt"Get text" sel:t>R t t timeout:5
tool url"Get URL" >R t t timeout:5

login u:t p:t>R t t;nav! "https://app.example.com/login";fill! "[name=email]" u;fill! "[name=password]" p;click! "[type=submit]";wait! ".dashboard";url!()
```

That's a complete login + assertion in one line, ~30 tokens. Equivalent Playwright JS:

```javascript
await page.goto('https://app.example.com/login');
await page.fill('[name=email]', user);
await page.fill('[name=password]', pass);
await page.click('[type=submit]');
await page.waitForSelector('.dashboard');
return page.url();
```

~45 tokens. ilo is 0.67x the tokens with the same semantics, plus verified error handling at every step via `!`.

### Example: scrape and validate

```
tool nav"Navigate" url:t>R _ t timeout:30
tool txt"Get text" sel:t>R t t timeout:5
tool vis"Visible?" sel:t>R b t timeout:5

chk url:t>R _ t;nav! url;t=txt! "h1";!has t "Welcome"{^+"Expected welcome, got: "t};v=vis! ".error-banner";v{^"Error banner visible"};~_
```

### Server implementation sketch (Node.js)

```javascript
// playwright-server.js — tool server for ilo
const { chromium } = require('playwright');

let browser, page;

async function init() {
  browser = await chromium.launch();
  const context = await browser.newContext();
  page = await context.newPage();
}

const handlers = {
  nav:    async ({ url }) => { await page.goto(url); },
  click:  async ({ sel }) => { await page.click(sel); },
  fill:   async ({ sel, val }) => { await page.fill(sel, val); },
  txt:    async ({ sel }) => await page.textContent(sel),
  vis:    async ({ sel }) => await page.isVisible(sel),
  wait:   async ({ sel }) => { await page.waitForSelector(sel); },
  shot:   async ({ path }) => { await page.screenshot({ path }); return path; },
  eval:   async ({ code }) => await page.evaluate(code),
  title:  async () => await page.title(),
  url:    async () => page.url(),
};

// stdio JSON-RPC loop
process.stdin.setEncoding('utf8');
let buffer = '';
process.stdin.on('data', async (chunk) => {
  buffer += chunk;
  // newline-delimited JSON
  let nl;
  while ((nl = buffer.indexOf('\n')) !== -1) {
    const line = buffer.slice(0, nl);
    buffer = buffer.slice(nl + 1);
    try {
      const { id, method, params } = JSON.parse(line);
      const handler = handlers[method];
      if (!handler) {
        process.stdout.write(JSON.stringify({ id, error: `unknown method: ${method}` }) + '\n');
        continue;
      }
      const result = await handler(params || {});
      process.stdout.write(JSON.stringify({ id, result: result ?? null }) + '\n');
    } catch (e) {
      process.stdout.write(JSON.stringify({ id: null, error: e.message }) + '\n');
    }
  }
});

init();
```

### What this needs from ilo

- **D1d: ToolProvider infrastructure** — the `StdioProvider` variant that spawns a child process and communicates via newline-delimited JSON over stdin/stdout
- **D1e: Value <-> JSON** — serialise tool args to JSON, deserialise results back to ilo values
- **No new language features** — everything works with existing `tool` declarations, `!` auto-unwrap, `?` matching, and `R` result types

### Advantages

- Works today (once D1d is done) — no new language primitives needed
- Full Playwright power — all browsers, all features, maintained by Microsoft
- Small tool surface — ~15 tool declarations cover 90% of browser automation
- Agent writes terse ilo, heavy lifting is in the Node.js server
- Server can be swapped (Puppeteer, Selenium, etc.) without changing ilo code

### Disadvantages

- Requires Node.js + Playwright installed alongside ilo
- Latency: ilo → stdio → Node.js → CDP → browser (extra hop vs direct CDP)
- State management: server must track page/context state across calls
- Can't express complex Playwright patterns (custom selectors, route handlers, multi-page coordination) without more tools

---

## Approach 2: Native CDP client

ilo speaks Chrome DevTools Protocol directly. No Node.js dependency.

### Architecture

```
ilo program  →  WebSocket (G2)  →  Chromium (CDP)
         └──→  spawn (G3)  →  chromium --remote-debugging-port=9222
```

### What CDP looks like

CDP is JSON-RPC over WebSocket. Example: navigate to a URL.

Request:
```json
{"id": 1, "method": "Page.navigate", "params": {"url": "https://example.com"}}
```

Response:
```json
{"id": 1, "result": {"frameId": "ABC123", "loaderId": "DEF456"}}
```

Event (async, pushed by browser):
```json
{"method": "Page.loadEventFired", "params": {"timestamp": 1234567890.123}}
```

### What ilo would need

```
-- launch browser
type cdp{ws:ws;id:n}

launch>R cdp t
  ;h=spawn! "chromium" "--headless --remote-debugging-port=9222"
  ;c=ws! "ws://127.0.0.1:9222/devtools/page/..."
  ;~cdp ws:c id:0

-- send CDP command and wait for response
cdp-call c:cdp method:t params:t>R t t
  ;nid=+c.id 1
  ;msg=++++"{\"id\":" (str nid) ",\"method\":\"" method "\",\"params\":" params "}"
  ;ws-send! c.ws msg
  ;r=ws-recv! c.ws
  ;~r

-- navigate
nav c:cdp url:t>R t t
  ;p=++"{\"url\":\"" url "\"}"
  ;cdp-call c "Page.navigate" p
```

### Problems with this approach

**1. String building is brutal in ilo.** Constructing JSON by hand with `+` concatenation is error-prone and token-expensive. ilo has no string interpolation, no template literals. Every CDP message becomes a wall of `+` operators.

**2. CDP is event-driven.** After `Page.navigate`, you need to listen for `Page.loadEventFired`. That's an async event pushed by the browser at an unpredictable time. ilo has no event model — `ws-recv` blocks, so you'd need a polling loop:

```
wait-load c:cdp>R _ t;r=ws-recv c.ws;?r{^e:^e;~msg:has msg "loadEventFired"{~_}};wait-load c
```

This is recursive polling — fragile, doesn't handle interleaved messages, and will stack overflow on slow pages.

**3. CDP is enormous.** The protocol has ~80 domains with ~500 methods and ~300 event types. Even a minimal browser automation needs:
- `Page.navigate`, `Page.reload`
- `Runtime.evaluate` (run JS in page)
- `DOM.querySelector`, `DOM.getDocument`
- `Input.dispatchMouseEvent`, `Input.dispatchKeyEvent`
- `Page.captureScreenshot` (returns base64 PNG)
- `Network.enable`, `Network.requestWillBeSent` (request interception)

That's ~20 methods minimum, each needing JSON construction, response parsing, and event handling.

**4. No JSON parsing.** CDP responses are JSON. ilo can't parse JSON — there's no `json-get key obj` builtin, no map type (yet). You'd get raw text back and have no way to extract fields.

**5. Binary data.** Screenshots come back as base64-encoded PNGs in JSON. ilo has no base64 decode, no binary write.

### When this approach makes sense

Only after:
- G2 (WebSocket) — bidirectional communication
- G3 (process spawning) — launch browser
- G4 (async) — handle CDP events without blocking
- E4 (maps) — parse JSON responses
- A JSON builtin or tool — `json-get key text > R t t`
- A base64/binary story — at minimum `b64decode text > bytes`, `write-file path bytes`

That's a lot of prerequisites. Could be 6+ months of work.

### Advantages (if built)

- Zero external dependencies — just ilo + chromium binary
- Fastest possible path (no Node.js overhead)
- Full control over CDP — can do things Playwright abstracts away
- Aligns with ilo's self-contained philosophy

### Disadvantages

- Massive implementation effort
- Reinvents what Playwright already solves
- Chromium-only (Firefox and WebKit use different protocols)
- CDP is a moving target — Chrome updates break things

---

## Recommendation

**Start with Approach 1 (tool server).** It's practical, works with existing ilo features (once D1d lands), and gives agents full browser automation immediately. The tool surface is small (~15 declarations) and the server is <100 lines of Node.js.

**Approach 2 is a long-term possibility** gated on G2-G4 + E4. If ilo's networking primitives mature enough, a native CDP client becomes feasible — but it's months of work and the ROI vs the tool server is marginal for most use cases.

The hybrid path: start with Approach 1, gradually move individual operations native as G2/G3/G4 land, and eventually the tool server becomes optional.

## Token efficiency comparison

| Framework | Login test | Tokens | vs Playwright |
|-----------|-----------|--------|---------------|
| Playwright (JS) | 6 lines, await chains | ~45 | 1.00x |
| Cypress (JS) | 6 lines, cy chains | ~50 | 1.11x |
| ilo + tool server | 1 line, `!` chains | ~30 | 0.67x |
| ilo + native CDP | 1 line but JSON construction | ~60 | 1.33x |

The tool server wrapper is actually more token-efficient than native CDP because the tool server handles JSON construction and CDP complexity internally.

## Multi-page and multi-context patterns

For tests needing multiple pages or browser contexts, the tool server can expose context management:

```
tool new-ctx"New browser context" >R t t timeout:10
tool new-page"New page in context" ctx:t>R t t timeout:10
tool use-page"Switch active page" pid:t>R _ t timeout:5
tool close-page"Close page" pid:t>R _ t timeout:5

-- multi-tab test
multi>R _ t;c=new-ctx!();p1=new-page! c;p2=new-page! c;use-page! p1;nav! "https://a.com";use-page! p2;nav! "https://b.com";close-page! p2;~_
```

Page/context IDs are opaque text handles — the server tracks the mapping internally.

## Network interception

```
tool route"Intercept network requests" pattern:t handler:t>R _ t timeout:5
tool unroute"Remove interception" pattern:t>R _ t timeout:5

-- mock API responses
mock>R _ t;route! "**/api/user" "{\"name\":\"test\"}";nav! "https://app.com";unroute! "**api/user";~_
```

The `handler` param is a JSON string describing the mock response. Complex routing logic stays in the server.

## Next steps

1. **Build D1d (ToolProvider)** — the stdio provider is the foundation
2. **Build the Playwright server** — ~100 lines of Node.js, 15 tool endpoints
3. **Write example ilo programs** — login test, scrape + validate, multi-page flow
4. **Benchmark token efficiency** — compare ilo + tools vs raw Playwright JS
5. **Iterate tool surface** — add/remove tools based on what agents actually need
