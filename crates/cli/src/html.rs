use deadcode_core::ScanReport;
use std::io;
use std::time::{SystemTime, UNIX_EPOCH};

/// Render a self-contained HTML report.
///
/// The scan data is embedded as JSON and rendered client-side, so the file
/// works from `file://` with no server. Tailwind comes from a CDN; if it
/// fails to load, the inline fallback below keeps the page dark and readable
/// rather than unstyled white.
pub fn render(report: &ScanReport) -> io::Result<String> {
    let data = serde_json::to_string(report)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?
        // A literal `</script>` inside the JSON would close the tag early.
        .replace("</", r"<\/");

    let generated_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);

    Ok(TEMPLATE
        .replace("__DATA__", &data)
        .replace("__GENERATED_AT__", &generated_at.to_string()))
}

const TEMPLATE: &str = r##"<!DOCTYPE html>
<html lang="en" class="dark">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>deadcode report</title>
<script src="https://cdn.jsdelivr.net/npm/@tailwindcss/browser@4"></script>
<style>
  /* Fallback so the page stays legible if the CDN is unreachable. */
  :root { color-scheme: dark; }
  body { background:#09090b; color:#e4e4e7; font-family: ui-sans-serif, system-ui, -apple-system, "Segoe UI", sans-serif; margin:0; }
  .mono { font-family: ui-monospace, SFMono-Regular, "SF Mono", Menlo, Consolas, monospace; }
  ::-webkit-scrollbar { width:10px; height:10px; }
  ::-webkit-scrollbar-thumb { background:#3f3f46; border-radius:6px; }
  ::-webkit-scrollbar-track { background:transparent; }
  [hidden] { display:none !important; }
</style>
</head>
<body class="bg-zinc-950 text-zinc-200 antialiased">

<div class="mx-auto max-w-6xl px-5 py-8">

  <header class="mb-8">
    <div class="flex flex-wrap items-start justify-between gap-3">
      <h1 class="text-2xl font-semibold tracking-tight text-zinc-100">
        dead<span class="text-emerald-400">code</span>
      </h1>
      <div class="flex flex-col items-end gap-2">
        <!-- Ko-fi widget. draw() uses document.write, so this must stay inline
             in the parse flow rather than being moved to the end of <body>. -->
        <div id="kofi" class="[&_img]:!inline [&_a]:!inline-block">
          <script type='text/javascript' src='https://storage.ko-fi.com/cdn/widget/Widget_2.js'></script>
          <script type='text/javascript'>
            if (window.kofiwidget2) {
              kofiwidget2.init('Buy a ko-fi', '#c71010', 'E6J7263PU9');
              kofiwidget2.draw();
            }
          </script>
        </div>
        <p class="text-xs text-zinc-500" id="generated"></p>
      </div>
    </div>
    <p class="mono mt-2 truncate text-sm text-zinc-400" id="root"></p>
    <p class="mt-1 text-sm text-zinc-500" id="counts"></p>
    <p class="mt-4 rounded-md border border-amber-900/50 bg-amber-950/30 px-3 py-2 text-xs text-amber-200/80">
      Heuristic results. No type information, no call graph. Reflection,
      codegen, and cross-module API are invisible to it. Treat every row as a
      candidate to inspect, never as a verdict.
    </p>
  </header>

  <section class="mb-6 grid grid-cols-1 gap-3 sm:grid-cols-3" id="cards"></section>

  <section class="mb-5 flex flex-wrap items-center gap-3">
    <input id="search" type="search" placeholder="Filter by name, file, or kind…"
      class="mono min-w-[16rem] flex-1 rounded-lg border border-zinc-800 bg-zinc-900/70 px-3 py-2 text-sm text-zinc-100 placeholder-zinc-600 outline-none focus:border-emerald-600/70 focus:ring-1 focus:ring-emerald-600/40">
    <select id="lang"
      class="rounded-lg border border-zinc-800 bg-zinc-900/70 px-3 py-2 text-sm text-zinc-300 outline-none focus:border-emerald-600/70">
      <option value="">All languages</option>
      <option value="swift">Swift</option>
      <option value="kotlin">Kotlin</option>
    </select>
    <label class="flex cursor-pointer select-none items-center gap-2 text-sm text-zinc-400">
      <input id="group" type="checkbox" class="h-4 w-4 accent-emerald-500" checked>
      Group by file
    </label>
  </section>

  <section id="results" class="space-y-2"></section>

  <p id="empty" class="hidden py-16 text-center text-sm text-zinc-500">
    Nothing matches the current filters.
  </p>

  <footer class="mt-12 border-t border-zinc-900 pt-5 text-xs text-zinc-600">
    Click any row to copy <span class="mono text-zinc-500">file:line</span>.
  </footer>
</div>

<script type="application/json" id="payload">__DATA__</script>
<script>
(function () {
  var report = JSON.parse(document.getElementById('payload').textContent);
  var findings = report.findings || [];
  var active = null; // null = all buckets

  var META = {
    dead:     { label: 'Dead',      note: 'No reference anywhere',                 dot: 'bg-rose-400',   ring: 'ring-rose-500/40',   text: 'text-rose-300' },
    testOnly: { label: 'Test-only', note: 'Production code never touches these',   dot: 'bg-amber-400',  ring: 'ring-amber-500/40',  text: 'text-amber-300' },
    dynamic:  { label: 'Dynamic?',  note: 'Only in a string literal or resource',  dot: 'bg-sky-400',    ring: 'ring-sky-500/40',    text: 'text-sky-300' }
  };
  var ORDER = ['dead', 'testOnly', 'dynamic'];

  function esc(s) {
    return String(s).replace(/[&<>"']/g, function (c) {
      return { '&':'&amp;', '<':'&lt;', '>':'&gt;', '"':'&quot;', "'":'&#39;' }[c];
    });
  }

  document.getElementById('root').textContent = report.root || '';
  document.getElementById('counts').textContent =
    (report.codeFiles || 0) + ' code files · ' +
    (report.resourceFiles || 0) + ' resource files · ' +
    (report.declarations || 0) + ' declarations examined · ' +
    findings.length + ' flagged';
  document.getElementById('generated').textContent =
    'Generated ' + new Date(__GENERATED_AT__).toLocaleString();

  // Summary cards double as bucket filters.
  var cards = document.getElementById('cards');
  cards.innerHTML = ORDER.map(function (b) {
    var m = META[b];
    var n = findings.filter(function (f) { return f.bucket === b; }).length;
    return '<button data-bucket="' + b + '" ' +
      'class="card group rounded-xl border border-zinc-800 bg-zinc-900/50 p-4 text-left transition hover:border-zinc-700 hover:bg-zinc-900">' +
        '<div class="flex items-center gap-2">' +
          '<span class="h-2 w-2 rounded-full ' + m.dot + '"></span>' +
          '<span class="text-sm font-medium text-zinc-300">' + m.label + '</span>' +
          '<span class="ml-auto text-2xl font-semibold tabular-nums ' + m.text + '">' + n + '</span>' +
        '</div>' +
        '<p class="mt-1 text-xs text-zinc-500">' + m.note + '</p>' +
      '</button>';
  }).join('');

  cards.addEventListener('click', function (e) {
    var btn = e.target.closest('[data-bucket]');
    if (!btn) return;
    active = (active === btn.dataset.bucket) ? null : btn.dataset.bucket;
    Array.prototype.forEach.call(cards.children, function (c) {
      var on = c.dataset.bucket === active;
      c.classList.toggle('ring-2', on);
      c.classList.toggle('border-zinc-600', on);
      ORDER.forEach(function (b) { c.classList.remove(META[b].ring); });
      if (on) c.classList.add(META[active].ring);
    });
    render();
  });

  function row(f) {
    var m = META[f.bucket] || META.dead;
    var refs = 'prod ' + f.prodRefs + ' · test ' + f.testRefs + ' · dyn ' + f.dynamicRefs;
    return '<div class="row flex cursor-pointer items-center gap-3 rounded-lg border border-zinc-800/70 bg-zinc-900/40 px-3 py-2 transition hover:border-zinc-700 hover:bg-zinc-900" ' +
             'data-copy="' + esc(f.file) + ':' + f.line + '" title="' + refs + '">' +
             '<span class="h-1.5 w-1.5 shrink-0 rounded-full ' + m.dot + '"></span>' +
             '<span class="mono truncate text-sm text-zinc-100">' + esc(f.name) + '</span>' +
             '<span class="shrink-0 rounded border border-zinc-700/70 px-1.5 py-0.5 text-[10px] uppercase tracking-wide text-zinc-400">' + esc(f.kind) + '</span>' +
             '<span class="mono ml-auto shrink-0 text-xs text-zinc-500">' + esc(f.file) + ':' + f.line + '</span>' +
           '</div>';
  }

  function render() {
    var q = document.getElementById('search').value.trim().toLowerCase();
    var lang = document.getElementById('lang').value;
    var grouped = document.getElementById('group').checked;

    var list = findings.filter(function (f) {
      if (active && f.bucket !== active) return false;
      if (lang && f.language !== lang) return false;
      if (!q) return true;
      return (f.name + ' ' + f.file + ' ' + f.kind).toLowerCase().indexOf(q) !== -1;
    });

    var out = document.getElementById('results');
    document.getElementById('empty').classList.toggle('hidden', list.length > 0);

    if (!grouped) {
      out.innerHTML = list.map(row).join('');
      return;
    }

    var byFile = {};
    list.forEach(function (f) { (byFile[f.file] = byFile[f.file] || []).push(f); });

    out.innerHTML = Object.keys(byFile).sort().map(function (file) {
      var items = byFile[file].sort(function (a, b) { return a.line - b.line; });
      return '<details open class="rounded-xl border border-zinc-800 bg-zinc-900/30">' +
        '<summary class="mono flex cursor-pointer items-center gap-2 px-3 py-2 text-sm text-zinc-300 hover:text-zinc-100">' +
          esc(file) +
          '<span class="ml-auto rounded bg-zinc-800 px-2 py-0.5 text-xs tabular-nums text-zinc-400">' + items.length + '</span>' +
        '</summary>' +
        '<div class="space-y-1 px-2 pb-2">' + items.map(row).join('') + '</div>' +
      '</details>';
    }).join('');
  }

  document.getElementById('results').addEventListener('click', function (e) {
    var el = e.target.closest('[data-copy]');
    if (!el || !navigator.clipboard) return;
    navigator.clipboard.writeText(el.dataset.copy).then(function () {
      el.classList.add('ring-1', 'ring-emerald-500/60');
      setTimeout(function () { el.classList.remove('ring-1', 'ring-emerald-500/60'); }, 600);
    });
  });

  ['search', 'lang', 'group'].forEach(function (id) {
    document.getElementById(id).addEventListener('input', render);
  });

  render();
})();
</script>
</body>
</html>
"##;
