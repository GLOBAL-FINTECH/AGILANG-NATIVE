# AGS Template Compilation Architecture

## Overview

AGS (AGILANG Reactive Server Templates) is the presentation layer for AGILANG applications. It compiles to both:

1. **Server-side rendering** — generates complete HTML at request time
2. **Browser hydration** — generates minimal JavaScript for reactive updates

---

## Compilation Pipeline

```
dashboard.ags
    ↓
AGS Lexer
    ├─ Template expressions {{ ... }}
    ├─ Directives @if, @for, @match
    ├─ Component tags <MyComponent />
    ├─ HTML tags
    └─ Script blocks <script ags>
    ↓
AGS Parser
    ├─ Validates template structure
    ├─ Builds AST
    ├─ Resolves component references
    └─ Analyzes data dependencies
    ↓
Type Checker
    ├─ Prop types from components
    ├─ Expression types
    ├─ Data flow analysis
    └─ Binding correctness
    ↓
Reactive Dependency Graph
    ├─ Which DOM nodes depend on state?
    ├─ Which expressions depend on props?
    ├─ Computed property dependencies
    └─ Event handler targets
    ↓
├─ Render IR (Server Target)
│   ├─ Serializable render instructions
│   ├─ Prop binding plan
│   ├─ Slot resolution plan
│   └─ Conditional rendering logic
│
└─ Reactive IR (Browser Target)
    ├─ State nodes
    ├─ Update graph
    ├─ Event bindings
    └─ DOM paths
    ↓
├─ Rust Renderer (SSR)
│   ├─ Generates HTML
│   ├─ Escapes text content
│   ├─ Fills slots
│   └─ Renders conditionals
│
└─ JavaScript/WASM Generator (Browser)
    ├─ Generates hydration code
    ├─ Wires event listeners
    ├─ Sets up reactive update
    └─ Connects API endpoints
```

---

## Server-Side Rendering (SSR)

### Compilation Target

**Input**: Parsed and type-checked AGS template  
**Output**: Typed Rust rendering plan

```rust
pub fn render_dashboard(
    ctx: &RenderContext,
    user: &User,
    transactions: &[Transaction],
) -> Result<String, RenderError> {
    let mut output = String::new();

    // Layout header
    output.push_str("<html><head>");
    output.push_str(&render_seo_meta(user));
    output.push_str("</head><body>");

    // Component rendering
    output.push_str(&render_app_layout(ctx, user, transactions)?);

    output.push_str("</body></html>");
    Ok(output)
}
```

### Key Features

#### 1. Safe HTML Rendering

```agi
@page "/dashboard"

<h1>Welcome, {{ user.name }}</h1>

<!-- AGS compiler automatically:
     1. Escapes user.name for XSS safety
     2. Detects encoding boundary
     3. Renders safe HTML
-->
```

**Compiler generates**:

```rust
output.push_str("<h1>Welcome, ");
output.push_str(&html_escape(user.name));
output.push_str("</h1>");
```

#### 2. Slot Rendering

```agi
@page "/dashboard"
@layout "layouts/app.ags"

<h1>Dashboard</h1>
<!-- Content is placed in @slot main -->
```

**Layout (layouts/app.ags)**:

```agi
<header>...</header>
<main>
    @slot main
</main>
<footer>...</footer>
```

**Compiler generates**:

```rust
render_header(ctx)?;
output.push_str("<main>\n");
output.push_str(&render_page_content(ctx)?);
output.push_str("</main>\n");
render_footer(ctx)?;
```

#### 3. Conditional Rendering

```agi
@if user.is_admin:
    <button>Delete User</button>
@else if user.is_moderator:
    <button>Suspend User</button>
@else:
    <button>Report User</button>
```

**Compiler generates**:

```rust
if user.is_admin {
    output.push_str("<button>Delete User</button>");
} else if user.is_moderator {
    output.push_str("<button>Suspend User</button>");
} else {
    output.push_str("<button>Report User</button>");
}
```

#### 4. Loop Rendering

```agi
<ul>
    @for tx in transactions:
        <li>
            {{ tx.reference }} — ${{ format_amount(tx.amount) }}
            <span class="date">{{ format_date(tx.date) }}</span>
        </li>
</ul>
```

**Compiler generates**:

```rust
output.push_str("<ul>");
for tx in transactions {
    output.push_str("<li>");
    output.push_str(&html_escape(&tx.reference));
    output.push_str(" — $");
    output.push_str(&format_amount(tx.amount));
    output.push_str("<span class=\"date\">");
    output.push_str(&format_date(tx.date));
    output.push_str("</span></li>");
}
output.push_str("</ul>");
```

#### 5. Component Rendering

```agi
<TransactionRow
    @for tx in transactions
    transaction:tx
    on:select="on_select_transaction"
/>
```

**Compiler generates**:

```rust
for tx in transactions {
    let row_html = render_transaction_row(
        &RenderContext { transaction: tx.clone() }
    )?;
    output.push_str(&row_html);
}
```

#### 6. Computed Properties

```agi
<script ags>
prop transactions: Array<Transaction>

computed visible:
    return transactions.filter(fn(tx):
        return tx.amount > 0
    )
</script>

<ul>
    @for tx in visible:
        <li>{{ tx.reference }}</li>
</ul>
```

**Compiler generates**:

```rust
let visible: Vec<Transaction> = transactions
    .iter()
    .filter(|tx| tx.amount > 0)
    .cloned()
    .collect();

// Render using `visible`
```

### SEO and Meta Tags

```agi
@page "/product/:id"

<script ags>
prop product: Product

prop og_title: string = product.name
prop og_image: string = product.image_url
prop og_description: string = product.description
</script>

<head>
    <meta property="og:title" content="{{ og_title }}" />
    <meta property="og:image" content="{{ og_image }}" />
    <meta property="og:description" content="{{ og_description }}" />
    <meta name="description" content="{{ og_description }}" />
    <title>{{ product.name }}</title>
</head>
```

**Compiler generates metadata** so search engines see complete, relevant HTML at first request.

---

## Browser Hydration

### Compilation Target

**Input**: Parsed and type-checked AGS template + reactive dependency graph  
**Output**: Generated JavaScript module for browser

```javascript
// Generated hydration code
const state = {
  filter: "",
  visible: null,
};

const nodes = {
  filterInput: document.querySelector("[data-agi-bind='filter']"),
  transactionList: document.querySelector("[data-agi-list='visible']"),
  itemTemplates: document.querySelectorAll("[data-agi-item='tx']"),
};

function updateVisible() {
  state.visible = state.transactions.filter((tx) =>
    tx.reference.includes(state.filter),
  );
  renderList(nodes.transactionList, state.visible);
}

function setFilter(value) {
  state.filter = value;
  updateVisible();
}

// Attach event listeners
nodes.filterInput.addEventListener("change", (e) => {
  setFilter(e.target.value);
});
```

### Reactive Update Strategy

**Goal**: Update only affected DOM nodes

```agi
<script ags>
state count: i32 = 0

fn increment():
    count = count + 1
</script>

<div>
    <p>Count: <span>{{ count }}</span></p>
    <button on:click="increment">+1</button>
</div>
```

**Compiler detects**:

- `count` state node
- `<span>` displays `{{ count }}`
- `<button>` triggers `increment`

**Generated JavaScript**:

```javascript
// Update only the span, not the whole page
function setCount(value) {
  count = value;
  countSpan.textContent = String(value); // Surgical update
}

function increment() {
  setCount(count + 1);
}
```

### Form Binding

```agi
<script ags>
prop user: User
state email: string = user.email

fn on_email_change(value: string):
    email = value

fn on_submit():
    api.POST("/user/email", { email: email })
</script>

<form on:submit="on_submit">
    <input
        type="email"
        bind:value="email"
        on:change="on_email_change"
    />
    <button type="submit">Save</button>
</form>
```

**Compiler generates**:

```javascript
function onEmailChange(value) {
  email = value;
  validateEmail(email); // Optional validation
  updateSaveButtonState();
}

function onSubmit(event) {
  event.preventDefault();
  fetch("/user/email", {
    method: "POST",
    body: JSON.stringify({ email }),
  })
    .then(/* ... */)
    .catch(/* ... */);
}
```

### WebSocket and Server-Sent Events

```agi
<script ags>
state messages: Array<Message> = []

fn connect_websocket():
    let ws = WebSocket.new("wss://api.example.com/messages")

    ws.on_message(fn(event):
        let msg = json.parse(event.data)
        messages.push(msg)
    )
</script>

<button on:click="connect_websocket">Connect</button>
<ul>
    @for msg in messages:
        <li>{{ msg.text }} ({{ msg.timestamp }})</li>
</ul>
```

**Compiler generates**:

```javascript
function connectWebsocket() {
  const ws = new WebSocket("wss://api.example.com/messages");

  ws.addEventListener("message", (event) => {
    const msg = JSON.parse(event.data);
    messages.push(msg);
    renderMessageList(messageList, messages); // Append only
  });
}
```

### Error Boundaries

```agi
@error fn on_render_error(error: Error):
    return render_error_fallback(error)

<TransactionList
    @try transactions
    @catch error
    @on_error="on_render_error"
/>
```

---

## Compiler Optimizations

### 1. Unused Prop Elimination

```agi
<script ags>
prop user: User
prop unused_data: string  <!-- Not referenced -->

fn render():
    return "Hello, {{ user.name }}"
</script>
```

**Warning**: `unused_data` is defined but not used in rendering

### 2. Computed Memoization

```agi
<script ags>
prop items: Array<Item>

computed filtered:
    // Cached: only recomputes if `items` changes
    return items.filter(...)

computed count:
    // Depends on `filtered`, not `items` directly
    return filtered.length
</script>
```

### 3. Render Path Splitting

```agi
<!-- Server renders this static section once -->
<header class="static">
    <h1>{{ app.name }}</h1>
</header>

<!-- Server renders, browser hydrates reactivity -->
<main>
    <input bind:value="filter" />
    @for item in filtered:
        <Item item:item />
</main>

<!-- Client-only, never SSR -->
<div client:only>
    <WebGLVisualization />
</div>
```

---

## Best Practices

### 1. Separate Concerns

```agi
<!-- Good: Logic in script block -->
<script ags>
prop user: User

computed is_premium:
    return user.subscription_level == "premium"

fn upgrade_subscription():
    api.POST("/upgrade", {})
</script>

<!-- Minimal template, maximum clarity -->
@if is_premium:
    <PremiumFeature />
```

### 2. Avoid Inline Logic

```agi
<!-- ❌ Bad: Complex logic in template -->
<div>
    @if user && user.profile && user.profile.settings &&
       user.profile.settings.notifications_enabled:
        Show notification button
    @else:
        Show settings link
</div>

<!-- ✅ Good: Logic in script block -->
<script ags>
computed should_show_notifications:
    return user?.profile?.settings?.notifications_enabled ?? false
</script>

<div>
    @if should_show_notifications:
        Show notification button
    @else:
        Show settings link
</div>
```

### 3. Compose with Components

```agi
<!-- ❌ Avoid repetition -->
<div>
    <form on:submit="on_submit">
        <input bind:value="name" />
        <textarea bind:value="bio" />
        <button>Save</button>
    </form>
</div>

<!-- ✅ Create reusable component -->
<ProfileForm user:user on:save="on_save" />
```

### 4. Type All Props and State

```agi
<script ags>
prop user: User                    <!-- ✅ Typed prop -->
prop optional_data: Option<Data>   <!-- ✅ Optional types -->
state counter: i32 = 0             <!-- ✅ Typed state -->

fn increment() -> i32:             <!-- ✅ Typed return -->
    counter = counter + 1
    return counter
</script>
```

---

## Summary

AGS compilation is a **two-target process**:

1. **Server Target** → Complete, SEO-friendly HTML rendered at request time
2. **Browser Target** → Minimal, reactive JavaScript for interactive features

Together, they provide:

- ✅ Fast first-page load (SEO-optimized HTML)
- ✅ Instant interactivity (no full re-render)
- ✅ Type safety (same types in server and browser)
- ✅ Single source of truth (one `.ags` file, multiple outputs)

---

**Document Version**: 1.0  
**Effective Date**: 2026-07-21  
**Status**: Recommended AGS compilation practices
