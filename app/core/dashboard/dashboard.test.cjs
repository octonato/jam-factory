const assert = require('node:assert/strict')
const { readFileSync } = require('node:fs')
const { test } = require('node:test')
const vm = require('node:vm')

const html = readFileSync(`${__dirname}/dashboard.html`, 'utf8')
const scripts = [...html.matchAll(/<script(?:\s[^>]*)?>([\s\S]*?)<\/script>/g)]
  .map((match) => match[1])
  .filter((script) => script.trim() !== '')
assert.equal(scripts.length, 1, 'expected one inline dashboard script')

// Keep the real initialization and handlers, omitting only the network poll and its timer.
const polling = '\npoll()\nsetInterval(poll, POLL_MS)\n'
assert.ok(scripts[0].endsWith(polling), 'dashboard polling startup changed')
const source = scripts[0].slice(0, -polling.length)

class Target {
  listeners = new Map()

  addEventListener(type, listener, options = {}) {
    const entries = this.listeners.get(type) ?? []
    entries.push({ listener, once: options.once })
    this.listeners.set(type, entries)
  }

  removeEventListener(type, listener) {
    this.listeners.set(type, (this.listeners.get(type) ?? []).filter((entry) => entry.listener !== listener))
  }

  dispatch(type, properties = {}) {
    for (const { listener, once } of [...(this.listeners.get(type) ?? [])]) {
      listener({ currentTarget: this, ...properties })
      if (once) this.removeEventListener(type, listener)
    }
  }
}

// Returns the `.row.card` rows of `html`, each with its decoded `data-dir`.
function cardRows(html) {
  return [...html.matchAll(/class="row card[^"]*" data-dir="([^"]*)"/g)].map((match) => ({
    dataset: { dir: match[1].replace(/&#(\d+);/g, (_, code) => String.fromCharCode(code)) },
    classList: { toggle: () => {} },
    scrollIntoView: () => {},
  }))
}

function dashboard({ script = source, pageWidth = 1200, pageLeft = 0, splitterWidth = 6, fetcher, storage = new Map() } = {}) {
  const elements = new Map()
  const calls = []
  const geometry = { pageWidth, pageLeft, splitterWidth }

  // Moves each of `nodes` from its parent to the end of the children of `parent`.
  const appendTo = (parent, nodes) => {
    for (const node of nodes) {
      node.remove?.()
      node.parentElement = parent
      parent.children = [...parent.children, node]
    }
  }

  const element = (id) => {
    const properties = new Map()
    const classes = new Set()
    let html = ''
    const el = Object.assign(new Target(), {
      id,
      style: {
        setProperty: (name, value) => properties.set(name, value),
        getPropertyValue: (name) => properties.get(name) ?? '',
      },
      classList: {
        add: (name) => classes.add(name),
        remove: (name) => classes.delete(name),
        toggle: (name, force = !classes.has(name)) => {
          if (force) classes.add(name)
          else classes.delete(name)
          return force
        },
        contains: (name) => classes.has(name),
      },
      setPointerCapture: () => {},
      value: '',
      selected: false,
      focused: false,
      focus: () => { el.focused = true },
      blur: () => { el.focused = false },
      select: () => { el.selected = true },
      querySelector: () => null,
      querySelectorAll: () => [],
      closest: (selector) => (selector === `#${id}` ? el : el.parentElement?.closest(selector) ?? null),
      replaceChildren: (...nodes) => {
        html = ''
        el.children = nodes
        for (const node of nodes) node.parentElement = el
      },
      append: (...nodes) => appendTo(el, nodes),
      children: [],
      scrollTop: 0,
      firstElementChild: null,
      getBoundingClientRect: () => ({
        width: id === 'splitter' ? geometry.splitterWidth : geometry.pageWidth,
        left: geometry.pageLeft,
      }),
    })
    Object.defineProperty(el, 'innerHTML', {
      configurable: true,
      get: () => html,
      set: (value) => {
        html = value
        el.children = []
        if (id === 'term-tabs') {
          elements.delete('term-rename')
          const rename = value.match(/ id="term-rename" value="([^"]*)"/)
          if (rename !== null) {
            const input = element('term-rename')
            input.parentElement = el
            input.value = rename[1].replace(/&#(\d+);/g, (_, code) => String.fromCharCode(code))
          }
        }
      },
    })
    elements.set(id, el)
    return el
  }

  // Returns a detached element of `tag` that takes the focus, holds children, and matches `tag` and the ids of its parents. A textarea turns CRLF and CR in its value into LF.
  const createElement = (tag) => {
    let value = ''
    const el = {
      tagName: tag.toUpperCase(),
      get value() { return value },
      set value(text) { value = tag === 'textarea' ? text.replace(/\r\n?/g, '\n') : text },
      focused: false,
      parentElement: null,
      children: [],
      focus: () => { el.focused = true },
      blur: () => { el.focused = false },
      replaceChildren: (...nodes) => {
        el.children = []
        appendTo(el, nodes)
      },
      append: (...nodes) => appendTo(el, nodes),
      remove: () => {
        if (el.parentElement) el.parentElement.children = el.parentElement.children.filter((child) => child !== el)
        el.parentElement = null
      },
      closest: (selector) => (selector.split(',').some((part) => part.trim() === tag) ? el : el.parentElement?.closest(selector) ?? null),
    }
    return el
  }

  for (const id of ['tabs', 'page', 'term-store', 'status', 'problems', 'font-down', 'font-up', 'font-value', 'modal', 'modal-text', 'modal-cancel']) element(id)
  elements.get('modal').showModal = () => { elements.get('modal').open = true }

  let pageHtml = ''
  elements.get('page').querySelectorAll = () => cardRows(pageHtml)
  Object.defineProperty(elements.get('page'), 'innerHTML', {
    get: () => pageHtml,
    set(value) {
      pageHtml = value
      this.firstElementChild = element(value.includes('class="session-page"') ? 'session-page' : 'list')
      for (const [, id] of value.matchAll(/ id="([^"]+)"/g)) {
        element(id).parentElement = this.firstElementChild
      }
    },
  })

  const document = Object.assign(new Target(), {
    getElementById: (id) => elements.get(id) ?? null,
    createElement,
    documentElement: { style: {} },
  })
  const window = new Target()
  const context = vm.createContext({
    document,
    window,
    location: { hash: '' },
    localStorage: {
      getItem: (key) => storage.get(key) ?? null,
      setItem: (key, value) => storage.set(key, String(value)),
    },
    fetch: async (url, options) => {
      calls.push(url)
      if (!fetcher) throw new Error(`unexpected fetch: ${url}`)
      return fetcher(url, options)
    },
  })
  const run = (code) => vm.runInContext(code, context, { filename: 'dashboard.html' })
  run(script)
  return { run, context, elements, window, calls, geometry }
}

const session = { org: 'org', repo: 'repo', branch: 'main', id: 'session', dir: 'api/file/org/repo/main/session/', flow: 'build', stage: 'implement', round: 0 }
const filesUrl = 'api/files/org/repo/main/session'
const response = (text, ok = true, { status = ok ? 200 : 500, headers = {} } = {}) => ({
  ok,
  status,
  headers: new Headers(headers),
  text: async () => text,
})
const filesReply = (files) => response(JSON.stringify({ files }))
const sessionsReply = (sessions) => response(JSON.stringify({ sessions }))
const plain = (value) => JSON.parse(JSON.stringify(value))

function modelGate(app) {
  const failures = Array.from(app.run('metroSelfCheck()'))
  assert.equal(failures.length, 0, `metro self-check failed: ${failures.join(', ')}`)
}

test('the actual dashboard model passes every self-check', () => {
  const app = dashboard()
  modelGate(app)
  assert.equal(app.calls.length, 0)
})

test('the model gate rejects integerOrNull returning zero', () => {
  const original = 'function integerOrNull(value) {\n  return Number.isInteger(value) ? value : null\n}'
  assert.ok(source.includes(original), 'integerOrNull mutation target changed')
  const script = source.replace(original, 'function integerOrNull(value) {\n  return 0\n}')
  assert.throws(() => modelGate(dashboard({ script })), /metro self-check failed:.*bad-types/)
})

test('rendering an old session lists files and never fetches an absent run log', async () => {
  const app = dashboard({ fetcher: () => filesReply(['session.json']) })
  app.context.session = session
  await app.run('renderSessionPage(session, null)')
  assert.deepEqual(app.calls, [filesUrl])
})

test('a listed run log is parsed after listing and tolerates a truncated line', async () => {
  const app = dashboard({
    fetcher: (url) => url.endsWith('runs.jsonl')
      ? response('{"stop":"spec","round":0,"agent":"writer","tokens":5}\n{"stop":"pl')
      : filesReply(['runs.jsonl']),
  })
  app.context.session = session
  await app.run('renderSessionPage(session, null)')
  assert.deepEqual(app.calls, [filesUrl, `${session.dir}runs.jsonl`])
  assert.match(app.elements.get('session-card').innerHTML, /5 agent tokens/)
  const runs = await app.run('loadRuns("a space/", ["runs.jsonl"])')
  assert.equal(runs.length, 1)
  assert.equal(runs[0].tokens, 5)
  assert.equal(app.calls.at(-1), 'a%20space/runs.jsonl')
})

test('run logs remain optional when fetching fails', async () => {
  for (const fetcher of [() => response('', false), () => { throw new Error('disconnected') }]) {
    const app = dashboard({ fetcher })
    assert.deepEqual(plain(await app.run('loadRuns("session/", ["runs.jsonl"])')), [])
    assert.deepEqual(app.calls, ['session/runs.jsonl'])
    assert.deepEqual(plain(await app.run('loadRuns("session/", [])')), [])
    assert.equal(app.calls.length, 1)
  }
})

test('splitter clamps dragging, resizing and reopening to the actual page and splitter widths', async () => {
  const app = dashboard({ pageWidth: 1200, pageLeft: 100, splitterWidth: 10, fetcher: () => response('') })
  app.context.session = session
  await app.run('renderSessionPage(session, null)')
  const splitter = app.elements.get('splitter')
  const left = () => Number.parseInt(app.elements.get('session-page').style.getPropertyValue('--left'))
  let prevented = false
  splitter.dispatch('pointerdown', { pointerId: 1, preventDefault: () => { prevented = true } })
  splitter.dispatch('pointermove', { clientX: 2000 })
  assert.ok(prevented)
  assert.equal(left(), 890)
  splitter.dispatch('pointermove', { clientX: 600 })
  assert.equal(left(), 500, 'drag accounts for the page offset')
  splitter.dispatch('pointermove', { clientX: 0 })
  assert.equal(left(), 320)
  splitter.dispatch('pointermove', { clientX: 2000 })
  splitter.dispatch('lostpointercapture')
  splitter.dispatch('pointermove', { clientX: 0 })
  assert.equal(left(), 890, 'drag stops when pointer capture is lost')

  app.geometry.pageWidth = 900
  app.window.dispatch('resize')
  assert.equal(left(), 590)
  app.geometry.pageWidth = 600
  app.window.dispatch('resize')
  assert.equal(left(), 290, 'viewer reservation wins when both column minima cannot fit')

  app.geometry.splitterWidth = 0
  app.geometry.pageWidth = 400
  app.window.dispatch('resize')
  assert.equal(left(), 290, 'mobile layout keeps the saved desktop width')
  assert.match(html, /@media \(max-width: 800px\)\s*\{\s*\.session-page \{ grid-template-columns: minmax\(0, 1fr\); grid-template-rows: auto minmax\(0, 55fr\) minmax\(0, 45fr\); \}/)

  app.elements.get('page').innerHTML = '<div>list</div>'
  app.geometry.pageWidth = 850
  app.geometry.splitterWidth = 10
  app.window.dispatch('resize')
  app.run('leftWidth = 1200')
  await app.run('renderSessionPage(session, null)')
  assert.equal(left(), 540)
  assert.ok(app.geometry.pageWidth - left() - app.geometry.splitterWidth >= 300)
})

test('window resizing before opening a session is safe', () => {
  dashboard().window.dispatch('resize')
})

test('the session card heads the session page above the split panes', async () => {
  const app = dashboard({ fetcher: () => filesReply(['session.json']) })
  app.context.session = session
  await app.run('renderSessionPage(session, null)')

  const page = app.elements.get('page').innerHTML
  assert.match(page, /<div class="session-page">\s*<div class="session-bar" id="session-card"><\/div>\s*<section id="metro-pane">\s*<div class="metro" id="metro"><\/div>\s*<\/section>/)
  assert.match(html, /\.session-bar \{[^}]*grid-column: 1 \/ -1;/)
  assert.doesNotMatch(html, /session-card-wrap|position: sticky/)

  const card = app.elements.get('session-card').innerHTML
  assert.match(card, /<div class="name">\s*<div class="id">[^<]*<\/div>\s*<div class="path">[^<]*<\/div>\s*<\/div>\s*<div class="facts">/)
})

test('revealing a stop scrolls its card into view without a header offset', async () => {
  const app = dashboard({ fetcher: () => filesReply(['session.json']) })
  app.context.session = session
  await app.run('renderSessionPage(session, null)')

  const card = { style: {}, scrolled: null, scrollIntoView(options) { this.scrolled = options } }
  app.elements.get('metro').querySelector = () => card
  app.run('revealStop(0)')

  assert.equal(card.style.scrollMarginTop, undefined)
  assert.deepEqual(plain(card.scrolled), { block: 'nearest' })
})

test('Request timing is hidden in every flow, including legacy zero and derived durations', () => {
  const app = dashboard()
  for (const flow of ['build', 'review', 'vibe']) {
    for (const durationMs of [0, 5000, null]) {
      app.context.session = { ...session, flow }
      app.context.entry = { stop: 'request', round: 0, durationMs, started: '2026-10-22T10:00:00Z', ended: '2026-10-22T10:01:00Z' }
      const card = app.run('stopCardHtml(buildStops(session, ["request.md"], parseRuns(JSON.stringify(entry)))[0], null, false)')
      assert.doesNotMatch(card, /class="when"/)
    }
  }
  const card = app.run('stopCardHtml(withRuns(newStop("implement", 1, null, []), parseRuns(\'{"stop":"implement","round":1,"durationMs":5000}\')), null, false)')
  assert.match(card, /class="when">5s/)
})

test('session facts include stage, last round, deferred gaps and only recorded agent tokens', () => {
  const app = dashboard()
  app.context.session = { ...session, stage: 'done', round: 2, lastRound: true, counts: { deferred: 5, issued: 2 } }
  app.context.entries = [
    { stop: 'report', round: 1, agent: 'main', tokens: 9000 },
    { stop: 'implement', round: 1, agent: 'worker', tokens: 10 },
    { stop: 'review', round: 1, agent: 'reviewer', tokens: 5 },
    { stop: 'review', round: 1, agent: null, tokens: 1000 },
    { stop: 'triage', round: 1, agent: 'triager', tokens: null },
  ]
  const card = app.run('sessionCardHtml(session, parseRuns(entries.map(JSON.stringify).join("\\n")))')
  for (const fact of ['done', 'round 2', 'last round', '3 deferred without issue', '15 agent tokens']) {
    assert.ok(card.includes(`>${fact}</span>`), `missing session fact: ${fact}`)
  }
  app.context.session = { ...session, stage: 'blocked', blocked: { stage: 'implement' } }
  assert.match(app.run('sessionCardHtml(session, [])'), />blocked at implement<\/span>/)
  assert.doesNotMatch(app.run('sessionCardHtml(session, parseRuns(JSON.stringify(entries[0])))'), /agent tokens/)
  assert.doesNotMatch(app.run('sessionCardHtml(session, parseRuns(JSON.stringify(entries[4])))'), /agent tokens/)
  assert.match(app.run('sessionCardHtml(session, parseRuns(\'{"stop":"spec","round":0,"agent":"worker","tokens":0}\'))'), /0 agent tokens/)
})

test('blocked Implement keeps existing tasks as a link and session.json as its main file', () => {
  const app = dashboard()
  for (const flow of ['build', 'vibe']) {
    app.context.session = { ...session, flow, stage: 'blocked', blocked: { stage: 'implement', reason: 'needs input' } }
    const stop = app.run('buildStops(session, ["tasks.md"], []).find((stop) => stop.state === "blocked")')
    assert.equal(stop.file, 'session.json')
    assert.equal(stop.note, 'needs input')
    assert.deepEqual(plain(stop.links), [{ label: 'tasks', path: 'tasks.md' }])
    assert.deepEqual(plain(app.run('buildStops(session, [], []).find((stop) => stop.state === "blocked").links')), [])
  }
})

test('review rounds close on successful comments without submit.md and retain submit.md compatibility', () => {
  const app = dashboard()
  app.context.session = { flow: 'review', stage: 'verify', round: 1 }
  assert.equal(app.run('openRound(session, [], parseRuns(\'{"stop":"comment","round":1,"outcome":"done"}\'))'), 2)
  assert.equal(app.run('openRound(session, [], parseRuns(\'{"stop":"comment","round":1,"outcome":"failed"}\'))'), 1)
  assert.equal(app.run('openRound(session, ["round-1/submit.md"], [])'), 2)
})

test('finalize skips the following Implement while the next-round outcome retains it', () => {
  const app = dashboard()
  app.context.session = { ...session, stage: 'verify', round: 1 }
  assert.equal(app.run('buildStops(session, ["round-1/triage.md"], [], { 1: "finalize" }).some((stop) => stop.kind === "implement" && stop.round === 2)'), false)
  assert.equal(app.run('buildStops(session, ["round-1/triage.md"], [], { 1: "next round" }).some((stop) => stop.kind === "implement" && stop.round === 2)'), true)
})

test('rendering a session reads each round triage outcome and skips Implement after finalize', async () => {
  const pages = {
    [filesUrl]: JSON.stringify({ files: ['round-1/triage.md'] }),
    [`${session.dir}round-1/triage.md`]: '# Triage: round 1\n\n- F1: dismiss → D1\n\noutcome: finalize\n',
  }
  const app = dashboard({ fetcher: (url) => response(pages[url] ?? '', Object.hasOwn(pages, url)) })
  app.context.session = { ...session, stage: 'verify', round: 1 }
  await app.run('renderSessionPage(session, null)')

  const stops = app.run('sessionView.stops')
  assert.equal(stops.some((stop) => stop.kind === 'implement' && stop.round === 2), false, 'Implement shown after finalize')
  assert.equal(stops.find((stop) => stop.kind === 'review' && stop.round === 2)?.state, 'current')
  assert.ok(app.calls.includes(`${session.dir}round-1/triage.md`), 'triage outcome was not fetched')
})

test('parallel Review time spans all runs and falls back to durations when any timestamp is missing or invalid', () => {
  const app = dashboard()
  app.context.entries = [
    { stop: 'review', round: 1, started: '2026-10-22T10:00:00Z', ended: '2026-10-22T10:05:00Z', durationMs: null },
    { stop: 'review', round: 1, started: '2026-10-22T10:01:00Z', ended: '2026-10-22T10:09:00Z', durationMs: 480000 },
    { stop: 'report', round: 1, started: '2026-10-22T10:09:00Z', ended: '2026-10-22T10:09:45Z', durationMs: 45000 },
  ]
  const review = () => app.run('reviewStop(1, [], parseRuns(entries.map(JSON.stringify).join("\\n")), ["review", "report"])')
  assert.equal(review().durationMs, 585000)
  assert.match(app.run('stopCardHtml(reviewStop(1, [], parseRuns(entries.map(JSON.stringify).join("\\n")), ["review", "report"]), null, false)'), /9m 45s/)
  for (const started of [null, 'invalid']) {
    app.context.entries[1].started = started
    assert.equal(review().durationMs, 825000)
  }
})

test('loads sessions from the api in one call', async () => {
  const entries = [
    { org: 'a', repo: 'a b%c', branch: 'fix', session: 's1', category: 'walk', data: { repo: 'other', id: 'other', stage: 'implement' } },
    { org: 'b', repo: 'repo', branch: 'main', session: 's2', category: 'session', data: { stage: 'spec' } },
  ]
  const app = dashboard({ fetcher: (url) => (url === 'api/sessions' ? sessionsReply(entries) : filesReply([])) })
  const { sessions, problems } = plain(await app.run('loadSessions()'))

  assert.deepEqual(app.calls, ['api/sessions'])
  assert.deepEqual(problems, [])
  assert.deepEqual(
    sessions.map(({ org, repo, branch, id, category, dir, stage }) => ({ org, repo, branch, id, category, dir, stage })),
    [
      { org: 'a', repo: 'a b%c', branch: 'fix', id: 's1', category: 'walk', dir: 'api/file/a/a b%c/fix/s1/', stage: 'implement' },
      { org: 'b', repo: 'repo', branch: 'main', id: 's2', category: 'session', dir: 'api/file/b/repo/main/s2/', stage: 'spec' },
    ],
  )

  app.context.loaded = sessions
  await app.run('loadRuns(loaded[0].dir, ["runs.jsonl"])')
  await app.run('listFiles(loaded[0])')
  assert.deepEqual(app.calls.slice(1), ['api/file/a/a%20b%25c/fix/s1/runs.jsonl', 'api/files/a/a%20b%25c/fix/s1'])
})

test('lists a broken session as a problem', async () => {
  const entries = [
    { org: 'a', repo: 'repo', branch: 'main', session: 'broken', category: 'session', error: 'expected value at line 1 column 1' },
    { org: 'a', repo: 'repo', branch: 'main', session: 'fine', category: 'session', data: { flow: 'build', stage: 'spec' } },
    { org: 'b', repo: 'repo', branch: 'main', session: 'odd', category: 'session', data: { flow: 'build', stage: 'nope' } },
  ]
  const app = dashboard({ fetcher: () => sessionsReply(entries) })
  const { sessions, problems } = plain(await app.run('loadSessions()'))

  assert.deepEqual(sessions.map((s) => s.id), ['fine', 'odd'])
  assert.deepEqual(problems, ['a/repo/main/broken: expected value at line 1 column 1'])

  await app.run('poll()')
  const shown = app.elements.get('problems').innerHTML
  assert.ok(shown.includes('<div>a/repo/main/broken: expected value at line 1 column 1</div>'), shown)
  assert.ok(shown.includes('<div>b/repo/main/odd: unknown stage &#34;nope&#34;</div>'), shown)
  assert.equal(app.elements.get('status').textContent, '2 sessions')
})

test('missing xterm files are listed as problems', () => {
  const missing = Array.from(dashboard().run('missingScripts()'))
  assert.ok(missing.includes('xterm.js not loaded'), missing.join(', '))
  assert.ok(missing.includes('addon-fit.js not loaded'), missing.join(', '))
})

test('missing highlight files are listed as problems', () => {
  const missing = Array.from(dashboard().run('missingScripts()'))
  assert.ok(missing.includes('highlight.min.js not loaded'), missing.join(', '))
  assert.ok(missing.includes('diff2html-ui-base.min.js not loaded'), missing.join(', '))
})

test('parses fence info strings', () => {
  const app = dashboard()
  assert.deepEqual(plain(app.run("parseInfo('rust path=a/b.rs lines=2-9')")), { lang: 'rust', path: 'a/b.rs', lines: '2-9' })
  assert.deepEqual(plain(app.run("parseInfo('diff')")), { lang: 'diff', path: null, lines: null })
  assert.deepEqual(plain(app.run("parseInfo('')")), { lang: '', path: null, lines: null })
})

const markedScript = readFileSync(`${__dirname}/marked.min.js`, 'utf8')
const highlightScript = readFileSync(`${__dirname}/highlight.min.js`, 'utf8')
const highlightScalaScript = readFileSync(`${__dirname}/highlight-scala.min.js`, 'utf8')

// Returns a dashboard with the real marked and highlight.js, a DOMPurify stub that returns its input, and empty stubs for the other page scripts.
function markdownDashboard(options) {
  const app = dashboard(options)
  app.run(markedScript)
  app.run(highlightScript)
  app.run(highlightScalaScript)
  Object.assign(app.context, {
    DOMPurify: { sanitize: (html) => html },
    Diff2HtmlUI: class {},
    Terminal: class {},
    FitAddon: {},
  })
  return app
}

const diffText = [
  'diff --git a/f.txt b/f.txt',
  'index 1111111..2222222 100644',
  '--- a/f.txt',
  '+++ b/f.txt',
  '@@ -1 +1 @@',
  '-old',
  '+new',
].join('\n')
const diffFence = `## Step 1\n\n\`\`\`diff\n${diffText}\n\`\`\`\n`

test('a diff fence becomes a diff block with a toggle', () => {
  const app = markdownDashboard()
  app.context.text = diffFence
  const html = app.run("fileHtml('steps/01.md', text)")

  assert.ok(html.includes('<div class="diff" data-diff="0"><div class="diff-bar">'), html)
  assert.ok(html.includes('<button type="button" data-layout="line-by-line" aria-pressed="true">Unified</button>'), html)
  assert.ok(html.includes('<button type="button" data-layout="side-by-side" aria-pressed="false">Split</button>'), html)
  assert.ok(html.includes('</div><div class="diff-body"></div></div>'), html)
  assert.deepEqual(Array.from(app.run('pendingDiffs')), [diffText])
})

test('a tagged code fence shows its path and lines', () => {
  const app = markdownDashboard()
  app.context.text = '```rust path=a/b.rs lines=2-9\nfn main() {}\n```\n'
  const html = app.run("fileHtml('steps/01.md', text)")

  assert.ok(html.includes('<div class="code"><div class="code-path">a/b.rs:2-9</div><pre><code class="hljs"><span class="hljs-keyword">fn</span>'), html)
})

test('a code fence in a known language shows highlight.js colours', () => {
  const app = markdownDashboard()
  app.context.text = "```javascript\nconst a = '<b>'\n```\n"
  const html = app.run("fileHtml('steps/01.md', text)")

  const code = '<span class="hljs-keyword">const</span> a = <span class="hljs-string">&#x27;&lt;b&gt;&#x27;</span>'
  assert.ok(html.includes(`<div class="code"><pre><code class="hljs">${code}</code></pre></div>`), html)
})

test('a code fence in an unknown language shows its text escaped', () => {
  const app = markdownDashboard()
  app.context.text = '```nope\n<b>\n```\n'
  const html = app.run("fileHtml('steps/01.md', text)")

  assert.ok(html.includes('<div class="code"><pre><code class="hljs">&#60;b&#62;'), html)
})

const excerptSource = readFileSync(`${__dirname}/../../mcp/src/excerpt.rs`, 'utf8')

// Returns the language names that `language` in `app/mcp/src/excerpt.rs` returns.
function excerptLanguages() {
  const start = excerptSource.indexOf('pub fn language(')
  const end = excerptSource.indexOf('\n}\n', start)
  return [...excerptSource.slice(start, end).matchAll(/=> "([^"]+)"/g)].map((match) => match[1])
}

test('every language an excerpt names resolves in highlight.js', () => {
  const app = markdownDashboard()
  const languages = excerptLanguages()

  assert.ok(languages.includes('scala'), languages.join(', '))
  assert.deepEqual(languages.filter((language) => !app.context.hljs.getLanguage(language)), [])
})

const diff2htmlScript = readFileSync(`${__dirname}/diff2html-ui-base.min.js`, 'utf8')

// Returns the HTML that `drawDiff` draws for `text` through the vendored diff2html.
function diff2htmlHtml(app, text) {
  const body = { innerHTML: '', querySelector: () => null, querySelectorAll: () => [] }
  app.context.block = { diffText: text, querySelector: () => body }
  app.run("drawDiff(block, 'line-by-line')")
  return body.innerHTML
}

test('a diff fence without file headers shows as diff code', () => {
  const app = markdownDashboard()
  app.run(diff2htmlScript)
  const languages = []
  app.context.hljs.highlight = (text, { language }) => {
    languages.push(language)
    return { value: text }
  }
  const hunk = '@@ -1 +1 @@\n-old\n+new'
  assert.ok(!diff2htmlHtml(app, hunk).includes('d2h-file-wrapper'))

  app.context.text = `\`\`\`diff\n${hunk}\n\`\`\`\n`
  const html = app.run("fileHtml('steps/01.md', text)")

  assert.ok(html.includes(`<div class="code"><pre><code class="hljs">${hunk}`), html)
  assert.ok(!html.includes('class="diff"'), html)
  assert.deepEqual(Array.from(app.run('pendingDiffs')), [])
  assert.deepEqual(languages, ['diff'])
})

test('a diff fence with file headers draws a file through diff2html', () => {
  const app = markdownDashboard()
  app.run(diff2htmlScript)
  const unified = ['--- a/f.txt', '+++ b/f.txt', '@@ -1 +1 @@', '-old', '+new'].join('\n')

  for (const text of [diffText, unified]) {
    app.context.text = `\`\`\`diff\n${text}\n\`\`\`\n`
    const html = app.run("fileHtml('steps/01.md', text)")

    assert.ok(html.includes('<div class="diff" data-diff="0">'), html)
    assert.deepEqual(Array.from(app.run('pendingDiffs')), [text])
    assert.ok(diff2htmlHtml(app, text).includes('d2h-file-wrapper'), text)
  }
})

test('parses routes with the org', () => {
  const app = dashboard()
  assert.deepEqual(
    plain(app.run('parseRoute("#/org/a%20b%25c/fix/s1")')),
    { kind: 'session', org: 'org', repo: 'a b%c', branch: 'fix', id: 's1', path: null },
  )
  assert.deepEqual(
    plain(app.run('parseRoute("#/org/repo/main/s1/round-1/report.md")')),
    { kind: 'session', org: 'org', repo: 'repo', branch: 'main', id: 's1', path: 'round-1/report.md' },
  )

  app.context.session = { org: 'org', repo: 'a b%c', branch: 'fix', id: 's1' }
  assert.equal(app.run('sessionHref(session)'), '#/org/a%20b%25c/fix/s1')
  assert.equal(app.run('fileHref(session, "round-1/report.md")'), '#/org/a%20b%25c/fix/s1/round-1/report.md')
  assert.equal(app.run('sessionHref(parseRoute(sessionHref(session)))'), '#/org/a%20b%25c/fix/s1')
})

test('parses the new routes', () => {
  const app = dashboard()
  const route = (hash) => plain(app.run(`parseRoute(${JSON.stringify(hash)})`))

  assert.deepEqual(route(''), { kind: 'repos' })
  assert.deepEqual(route('#/'), { kind: 'repos' })
  assert.deepEqual(route('#/sessions'), { kind: 'sessions' })
  assert.deepEqual(route('#/worktree/%2Fa%2Fb%20c'), { kind: 'worktree', path: '/a/b c' })
  assert.deepEqual(route('#/o/r/b/id/spec.md'), { kind: 'session', org: 'o', repo: 'r', branch: 'b', id: 'id', path: 'spec.md' })
})

test('a worktree route keeps a path with a space and a quote', () => {
  const app = dashboard()
  app.context.path = "/s/o/r/work tree's"

  assert.equal(app.run('worktreeHash(path)'), "#/worktree/%2Fs%2Fo%2Fr%2Fwork%20tree's")
  assert.deepEqual(plain(app.run('parseRoute(worktreeHash(path))')), { kind: 'worktree', path: "/s/o/r/work tree's" })
})

// Returns `html` with its `&#N;` entities decoded.
const unescapeHtml = (html) => html.replace(/&#(\d+);/g, (_, code) => String.fromCharCode(code))

// Returns the text of `html`, without its tags, with its entities decoded and its spaces collapsed.
const textOf = (html) => unescapeHtml(html.replace(/<[^>]*>/g, ' ')).replace(/\s+/g, ' ').trim()

const pageText = (app) => textOf(app.elements.get('page').innerHTML)

// Returns the entry of `session` in the `api/sessions` reply.
const sessionEntry = ({ org, repo, branch, id, dir, ...data }) => ({ org, repo, branch, session: id, category: 'session', data })

// Returns a dashboard on `hash` after one poll of an api that lists `sessions` and answers `api/repos` with `repos`.
async function reposApp(repos, sessions = [], hash = '#/') {
  const app = dashboard({
    fetcher: (url) => {
      if (url === 'api/repos') return response(JSON.stringify(repos))
      if (url === 'api/sessions') return sessionsReply(sessions.map(sessionEntry))
      return response('', false, { status: 404 })
    },
  })
  app.context.location.hash = hash
  await app.run('poll()')
  return app
}

// Returns the checkout headers of the repos page, each with its class and its text.
const repoHeaders = (app) => [...app.elements.get('repo-list').innerHTML.matchAll(/<h2 class="([^"]*)">([\s\S]*?)<\/h2>/g)]
  .map((match) => ({ class: match[1], text: textOf(match[2]) }))

// Returns the worktree rows of the repos page, each with its decoded link and its text.
const worktreeRows = (app) => [...app.elements.get('repo-list').innerHTML.matchAll(/<a class="body" href="([^"]*)">([\s\S]*?)<\/a>/g)]
  .map((match) => ({ href: unescapeHtml(match[1]), text: textOf(match[2]) }))

const listText = (app) => textOf(app.elements.get('repo-list').innerHTML)

// Types `text` into the filter field of the repos page.
function typeFilter(app, text) {
  const field = app.elements.get('repo-filter')
  field.value = text
  field.dispatch('input')
}

test('the repos page lists checkouts and worktree links', async () => {
  const linked = "/s/akka/sdk/work tree's"
  const repos = {
    error: null,
    repos: [
      {
        org: 'akka',
        repo: 'sdk',
        path: '/s/akka/sdk/main',
        error: null,
        worktrees: [
          { path: '/s/akka/sdk/main', branch: 'main', head: null },
          { path: linked, branch: null, head: '1234567' },
        ],
      },
      {
        org: 'octonato',
        repo: 'jam-factory',
        path: '/s/octonato/jam-factory/main',
        error: null,
        worktrees: [{ path: '/s/octonato/jam-factory/main', branch: 'fix/<b>', head: null }],
      },
    ],
  }
  const at = (id, worktree) => ({ ...session, id, worktree })
  const sessions = [at('s1', '/s/akka/sdk/main'), at('s2', '/s/akka/sdk/main/'), at('s3', linked), at('s4', '/s/akka/sdk')]
  const app = await reposApp(repos, sessions)

  assert.ok(app.calls.includes('api/repos'))
  assert.deepEqual(plain(app.run('reposState')), repos)
  assert.deepEqual(repoHeaders(app), [
    { class: 'org', text: 'akka/sdk akka/sdk/main' },
    { class: 'org', text: 'octonato/jam-factory octonato/jam-factory/main' },
  ])
  assert.deepEqual(worktreeRows(app), [
    { href: '#/worktree/%2Fs%2Fakka%2Fsdk%2Fmain', text: 'main akka/sdk/main 2 sessions' },
    { href: "#/worktree/%2Fs%2Fakka%2Fsdk%2Fwork%20tree's", text: "1234567 akka/sdk/work tree's 1 session" },
    { href: '#/worktree/%2Fs%2Foctonato%2Fjam-factory%2Fmain', text: 'fix/<b> octonato/jam-factory/main 0 sessions' },
  ])
  assert.doesNotMatch(app.elements.get('repo-list').innerHTML, /<b>/)
})

test('the repos page explains an empty config', async () => {
  const app = await reposApp({ error: null, repos: [] })

  assert.ok(listText(app).includes('~/.jam-factory/config.json'), listText(app))
  assert.ok(listText(app).includes('"repos"'), listText(app))
  assert.deepEqual(worktreeRows(app), [])
})

test('a checkout with an error is greyed with the reason', async () => {
  const repos = { error: null, repos: [{ org: 'o', repo: 'r', path: '/s/o/r/main', error: 'not a git repository', worktrees: [] }] }
  const app = await reposApp(repos)

  assert.deepEqual(repoHeaders(app), [{ class: 'org error', text: 'o/r o/r/main not a git repository' }])
  assert.deepEqual(worktreeRows(app), [])
  assert.match(html, /\.org-group \.org\.error \{[^}]*color: var\(--muted\);/)
})

test('a checkout without worktrees says so', async () => {
  const repos = { error: null, repos: [{ org: 'o', repo: 'r', path: '/s/o/r/main', error: null, worktrees: [] }] }
  const app = await reposApp(repos)

  assert.deepEqual(repoHeaders(app), [{ class: 'org', text: 'o/r o/r/main' }])
  assert.equal(listText(app), 'o/r o/r/main No worktrees.')
})

const filterRepos = {
  error: null,
  repos: [
    {
      org: 'akka',
      repo: 'sdk',
      path: '/s/akka/sdk/main',
      error: null,
      worktrees: [
        { path: '/s/akka/sdk/rate-limit', branch: 'rate-limit', head: null },
        { path: '/s/akka/sdk/wt', branch: 'feature', head: null },
      ],
    },
    { org: 'o', repo: 'r', path: '/s/o/r/main', error: null, worktrees: [{ path: '/s/o/r/feature', branch: 'feature', head: null }] },
  ],
}

// Returns the texts of the worktree rows of the repos page.
const rowTexts = (app) => worktreeRows(app).map((row) => row.text)

test('the filter shows the worktrees whose short path or branch holds every word', async () => {
  const app = await reposApp(filterRepos)

  typeFilter(app, 'FEAT')
  assert.deepEqual(rowTexts(app), ['feature akka/sdk/wt 0 sessions', 'feature o/r/feature 0 sessions'])
  assert.deepEqual(repoHeaders(app).map((header) => header.text), ['akka/sdk akka/sdk/main', 'o/r o/r/main'])

  typeFilter(app, 'sdk feat')
  assert.deepEqual(rowTexts(app), ['feature akka/sdk/wt 0 sessions'])
  assert.deepEqual(repoHeaders(app).map((header) => header.text), ['akka/sdk akka/sdk/main'])

  typeFilter(app, '  ')
  assert.deepEqual(rowTexts(app), [
    'rate-limit akka/sdk/rate-limit 0 sessions',
    'feature akka/sdk/wt 0 sessions',
    'feature o/r/feature 0 sessions',
  ])
})

test('a checkout whose name matches shows without a matching worktree', async () => {
  const repos = { error: null, repos: [...filterRepos.repos, { org: 'akka', repo: 'docs', path: '/s/akka/docs/main', error: null, worktrees: [] }] }
  const app = await reposApp(repos)

  typeFilter(app, 'docs')
  assert.deepEqual(repoHeaders(app), [{ class: 'org', text: 'akka/docs akka/docs/main' }])
  assert.equal(listText(app), 'akka/docs akka/docs/main No worktrees.')
})

test('a filter that hides everything says so', async () => {
  const app = await reposApp(filterRepos)

  typeFilter(app, 'zzz')
  assert.equal(listText(app), 'No worktree matches.')
})

test('a poll keeps the filter field', async () => {
  const app = await reposApp(filterRepos)
  const field = app.elements.get('repo-filter')

  typeFilter(app, 'sdk')
  await app.run('poll()')
  assert.equal(app.elements.get('repo-filter'), field)
  assert.equal(field.value, 'sdk')
  assert.deepEqual(rowTexts(app), ['rate-limit akka/sdk/rate-limit 0 sessions', 'feature akka/sdk/wt 0 sessions'])
})

test('the filter survives leaving and returning', async () => {
  const app = await reposApp(filterRepos)
  typeFilter(app, 'sdk')

  navigate(app, worktreeHashOf('/s/akka/sdk/wt'))
  assert.match(app.elements.get('page').innerHTML, /id="worktree-card"/)

  navigate(app, '#/')
  assert.equal(app.elements.get('repo-filter').value, 'sdk')
  assert.deepEqual(rowTexts(app), ['rate-limit akka/sdk/rate-limit 0 sessions', 'feature akka/sdk/wt 0 sessions'])
})

test('a config error is shown', async () => {
  const app = await reposApp({ error: 'bad', repos: [] })

  assert.match(listText(app), /\bbad\b/)
  assert.doesNotMatch(listText(app), /config\.json/)
})

test('a failed repos fetch keeps the sessions and shows the failure', async () => {
  const app = dashboard({
    fetcher: (url) => (url === 'api/sessions' ? sessionsReply([sessionEntry(session)]) : response('', false, { status: 500 })),
  })
  await app.run('poll()')

  assert.equal(app.elements.get('status').textContent, '1 sessions')
  assert.match(listText(app), /500 api\/repos/)
})

test('sessions match a worktree with or without a trailing slash', () => {
  const app = dashboard()
  const at = (id, worktree) => ({ ...session, id, dir: `api/file/org/repo/main/${id}/`, worktree })
  app.context.sessions = [at('a', '/w/main'), at('b', '/w/main/'), at('c', '/w/main2'), at('d', undefined), at('e', '/w')]
  app.run('state.sessions = sessions')
  const ids = (path) => Array.from(app.run(`sessionsOf(${JSON.stringify(path)})`), (s) => s.id)

  assert.deepEqual(ids('/w/main'), ['a', 'b'])
  assert.deepEqual(ids('/w/main/'), ['a', 'b'])
  assert.deepEqual(ids('/w'), ['e'])
  assert.equal(app.run('sameWorktree("/a//", "/a")'), false)
})

test('the sessions tab lists sessions', async () => {
  const app = await reposApp({ error: null, repos: [] }, [session], '#/sessions')
  const page = app.elements.get('page').innerHTML

  assert.deepEqual(cardRows(page).map((row) => row.dataset.dir), [session.dir])
  assert.match(page, /<h2 class="org">org<\/h2>/)
})

test('the tab strip marks the shown page', async () => {
  // Returns the tabs as their label and link, with `*` after the current one.
  const tabs = (app) => [...app.elements.get('tabs').innerHTML.matchAll(/<a class="tab" href="([^"]*)"( aria-current="page")?>([^<]*)<\/a>/g)]
    .map((match) => `${match[3]} ${match[1]}${match[2] ? '*' : ''}`)

  const app = await reposApp({ error: null, repos: [] }, [session])
  assert.deepEqual(tabs(app), ['Repos #/*', 'Sessions #/sessions'])

  const shown = [
    ['#/sessions', ['Repos #/', 'Sessions #/sessions*']],
    [app.run('sessionHref(state.sessions[0])'), ['Repos #/', 'Sessions #/sessions']],
    ['#/worktree/%2Fa', ['Repos #/', 'Sessions #/sessions']],
    ['', ['Repos #/*', 'Sessions #/sessions']],
  ]
  for (const [hash, expected] of shown) {
    app.context.location.hash = hash
    app.window.dispatch('hashchange')
    assert.deepEqual(tabs(app), expected, hash)
  }
})

test('groups sessions by org and filters them', () => {
  const at = (org, id, updated) => ({ ...session, org, id, dir: `api/file/${org}/repo/main/${id}/`, updated })
  const sessions = [at('b', 'b1', '2026-10-22T10:00:00Z'), at('a', 'a1', '2026-10-22T09:00:00Z'), at('a', 'a2', '2026-10-22T11:00:00Z')]
  const [b1, a1, a2] = sessions.map((s) => s.dir)

  // Returns the org headings and the session dirs of the list, in page order.
  const list = (app) => [...app.elements.get('page').innerHTML.matchAll(/<h2 class="org">([^<]*)<\/h2>|data-dir="([^"]*)"/g)]
    .map((match) => match[1] ?? match[2])

  // Returns the labels of the org filter, with `*` after the pressed one.
  const filter = (app) => [...app.elements.get('page').innerHTML.matchAll(/data-org="[^"]*" aria-pressed="(true|false)">([^<]*)</g)]
    .map((match) => `${match[2]}${match[1] === 'true' ? '*' : ''}`)

  const choose = (app, org) => app.elements.get('list').dispatch('click', {
    target: { closest: (selector) => (selector === 'button[data-org]' ? { dataset: { org } } : null) },
  })

  const storage = new Map()
  const app = dashboard({ storage })
  app.context.sessions = sessions
  app.run('state.sessions = sessions; renderList()')
  assert.deepEqual(list(app), ['a', a2, a1, 'b', b1])
  assert.deepEqual(filter(app), ['All*', 'a', 'b'])

  app.run('listKey("ArrowDown")')
  assert.equal(app.run('selectedDir'), a2)

  choose(app, 'b')
  assert.equal(storage.get('jam-board-org'), 'b')
  assert.deepEqual(list(app), ['b', b1])
  assert.deepEqual(filter(app), ['All', 'a', 'b*'])
  assert.equal(app.run('selectedDir'), null, 'the filter clears a hidden selection')

  app.run('listKey("ArrowDown"); listKey("ArrowDown")')
  assert.equal(app.run('selectedDir'), b1)
  app.run('listKey("ArrowUp")')
  assert.equal(app.run('selectedDir'), b1)
  assert.equal(app.run(`selectedDir = ${JSON.stringify(a2)}; listKey("Enter")`), false, 'a hidden session takes no key')

  const restored = dashboard({ storage })
  restored.context.sessions = sessions
  restored.run('state.sessions = sessions; renderList()')
  assert.deepEqual(list(restored), ['b', b1])

  restored.run('state.sessions = state.sessions.filter((s) => s.org === "a"); renderList()')
  assert.deepEqual(list(restored), ['a', a2, a1], 'a stored org without sessions shows every org')
  assert.deepEqual(filter(restored), ['All*', 'a'])

  choose(app, '')
  assert.equal(storage.get('jam-board-org'), '')
  assert.deepEqual(list(app), ['a', a2, a1, 'b', b1])

  app.run('state.sessions = []; renderList()')
  assert.deepEqual(filter(app), [])
  assert.match(app.elements.get('page').innerHTML, /No sessions yet\./)
})

test('archive and open post to the api routes with the org', async () => {
  const posts = []
  const app = dashboard({
    fetcher: (url, options) => {
      if (options?.method === 'POST') posts.push(url)
      return url === 'api/sessions' ? sessionsReply([]) : response('')
    },
  })
  app.context.messages = []
  app.context.session = { ...session, org: 'my org', repo: 'a b%c', id: 's1' }
  app.run('ask = async (message) => { messages.push(message); return true }')

  await app.run('archiveSession(session)')
  await app.run('openTool(session, "zed")')
  await new Promise(setImmediate)
  assert.deepEqual(posts, ['api/archive/my%20org/a%20b%25c/main/s1', 'api/open/zed/my%20org/a%20b%25c/main/s1'])
  assert.match(app.context.messages[0], /^Archive my org\/a b%c\/main\/s1\?/)
  assert.equal(app.context.location.hash, '#/sessions')

  const failing = dashboard({ fetcher: () => response('zed is missing', false) })
  failing.context.messages = []
  failing.context.session = app.context.session
  failing.run('tell = async (message) => { messages.push(message) }')
  await failing.run('openTool(session, "zed")')
  assert.deepEqual(failing.context.messages, ['Could not open my org/a b%c/main/s1 in Zed: zed is missing'])
})

test('the archive action draws an archive box', () => {
  const app = dashboard()
  const icon = app.run("CARD_ACTIONS.find((entry) => entry.action === 'archive').icon")
  assert.equal(icon, '<rect x="2" y="2.5" width="12" height="3" rx=".5"/><path d="M3 5.5v8h10v-8M6.5 8h3"/>')
})

// Dispatches a keydown for `key` with `modifiers` on `target` and returns true when the page took it.
function press(app, key, modifiers = {}, target = { closest: () => null }) {
  let prevented = false
  app.context.document.dispatch('keydown', {
    key,
    metaKey: false,
    ctrlKey: false,
    altKey: false,
    shiftKey: false,
    ...modifiers,
    target,
    preventDefault: () => { prevented = true },
  })
  return prevented
}

test('l, Escape and Cmd+[ return from a session to the list', () => {
  for (const [key, modifiers] of [['l'], ['Escape'], ['[', { metaKey: true }]]) {
    const app = dashboard()
    app.context.session = session
    app.run('state.sessions = [session]')
    app.context.location.hash = app.run('sessionHref(session)')

    assert.ok(press(app, key, modifiers), `${key} is taken`)
    assert.equal(app.context.location.hash, '#/sessions')
    assert.equal(app.run('selectedDir'), session.dir)
  }
})

test('l, Escape and Cmd+[ return from a worktree page to the repos', () => {
  for (const [key, modifiers] of [['l'], ['Escape'], ['[', { metaKey: true }]]) {
    const app = dashboard()
    app.context.location.hash = '#/worktree/%2Fa'

    assert.ok(press(app, key, modifiers), `${key} is taken`)
    assert.equal(app.context.location.hash, '#/')
  }
})

test('other modified keys, an open dialog, the list and the repos page ignore the back keys', () => {
  const app = dashboard()
  app.context.session = session
  app.run('state.sessions = [session]')
  const href = app.run('sessionHref(session)')
  app.context.location.hash = href

  for (const modifiers of [{ ctrlKey: true }, { metaKey: true, altKey: true }, { metaKey: true, shiftKey: true }]) {
    assert.equal(press(app, '[', modifiers), false)
  }
  assert.equal(press(app, 'l', { metaKey: true }), false)
  assert.equal(app.context.location.hash, href)

  app.elements.get('modal').open = true
  assert.equal(press(app, 'Escape'), false)
  assert.equal(app.context.location.hash, href)

  app.elements.get('modal').open = false
  app.context.location.hash = '#/sessions'
  assert.equal(press(app, 'Escape'), false)
  assert.equal(press(app, '[', { metaKey: true }), false)

  app.context.location.hash = '#/'
  assert.equal(press(app, 'Escape'), false)
  assert.equal(press(app, 'l'), false)
})

const [pathA, pathB, pathC] = ['/s/akka/sdk/alpha', '/s/akka/sdk/beta', '/s/o/r/gamma']
const selectRepos = {
  error: null,
  repos: [
    {
      org: 'akka',
      repo: 'sdk',
      path: '/s/akka/sdk/main',
      error: null,
      worktrees: [
        { path: pathA, branch: 'alpha', head: null },
        { path: pathB, branch: 'beta', head: null },
      ],
    },
    { org: 'o', repo: 'r', path: '/s/o/r/main', error: null, worktrees: [{ path: pathC, branch: 'gamma', head: null }] },
  ],
}

// Returns the decoded links of the outlined worktree rows of the repos page.
const selectedRows = (app) => [...app.elements.get('repo-list').innerHTML.matchAll(/<div class="row worktree selected">\s*<a class="body" href="([^"]*)">/g)]
  .map((match) => unescapeHtml(match[1]))

test('arrow keys move the worktree selection across checkouts and stop at the ends', async () => {
  const app = await reposApp(selectRepos)

  for (const path of [pathA, pathB, pathC, pathC]) {
    assert.ok(press(app, 'ArrowDown'))
    assert.equal(app.run('selectedWorktree'), path)
    assert.deepEqual(selectedRows(app), [worktreeHashOf(path)])
  }

  assert.ok(press(app, 'ArrowUp'))
  assert.equal(app.run('selectedWorktree'), pathB)
  assert.deepEqual(selectedRows(app), [worktreeHashOf(pathB)])

  const fresh = await reposApp(selectRepos)
  assert.ok(press(fresh, 'ArrowUp'))
  assert.equal(fresh.run('selectedWorktree'), pathC)
  assert.deepEqual(selectedRows(fresh), [worktreeHashOf(pathC)])
})

test('Enter opens the selected worktree', async () => {
  const app = await reposApp(selectRepos)

  assert.equal(press(app, 'Enter'), false)
  assert.equal(app.context.location.hash, '#/')

  press(app, 'ArrowDown')
  assert.ok(press(app, 'Enter'))
  assert.equal(app.context.location.hash, worktreeHashOf(pathA))
})

test('a filtered-out or removed worktree loses the selection', async () => {
  const repos = plain(selectRepos)
  const app = await reposApp(repos)

  press(app, 'ArrowDown')
  press(app, 'ArrowDown')
  assert.equal(app.run('selectedWorktree'), pathB)
  typeFilter(app, 'alpha')
  assert.equal(app.run('selectedWorktree'), null)
  assert.deepEqual(selectedRows(app), [])

  typeFilter(app, '')
  press(app, 'ArrowDown')
  assert.equal(app.run('selectedWorktree'), pathA)
  repos.repos[0].worktrees.shift()
  await app.run('poll()')
  assert.equal(app.run('selectedWorktree'), null)
  assert.deepEqual(selectedRows(app), [])
})

test('a poll keeps the selection', async () => {
  const app = await reposApp(selectRepos)
  press(app, 'ArrowDown')
  press(app, 'ArrowDown')

  await app.run('poll()')
  assert.equal(app.run('selectedWorktree'), pathB)
  assert.deepEqual(selectedRows(app), [worktreeHashOf(pathB)])
})

test('slash focuses the filter', async () => {
  const app = await reposApp(selectRepos)

  assert.ok(press(app, '/'))
  assert.equal(app.elements.get('repo-filter').focused, true)
})

test('keys in the filter field', async () => {
  const app = await reposApp(selectRepos)
  const field = app.elements.get('repo-filter')

  field.focus()
  assert.ok(press(app, 'ArrowDown', {}, field))
  assert.equal(field.focused, false)
  assert.equal(app.run('selectedWorktree'), pathA)

  field.focus()
  for (const key of ['t', 'l']) assert.equal(press(app, key, {}, field), false, `${key} is text`)
  assert.equal(field.focused, true)
  assert.equal(app.run('selectedWorktree'), pathA)
  assert.equal(app.context.location.hash, '#/')

  assert.ok(press(app, 'Escape', {}, field))
  assert.equal(field.focused, false)
  assert.equal(app.context.location.hash, '#/')

  press(app, 'ArrowDown')
  field.focus()
  assert.ok(press(app, 'Enter', {}, field))
  assert.equal(field.focused, false)
  assert.equal(app.context.location.hash, worktreeHashOf(pathB))

  const fresh = await reposApp(selectRepos)
  const freshField = fresh.elements.get('repo-filter')
  freshField.focus()
  assert.ok(press(fresh, 'Enter', {}, freshField))
  assert.equal(freshField.focused, false)
  assert.equal(fresh.run('selectedWorktree'), pathA)
  assert.equal(fresh.context.location.hash, worktreeHashOf(pathA))
})

test('a back key on a worktree page selects it on the repos page', async () => {
  const app = await reposApp(selectRepos, [], worktreeHashOf(pathB))

  assert.ok(press(app, 'Escape'))
  assert.equal(app.context.location.hash, '#/')
  assert.equal(app.run('selectedWorktree'), pathB)

  app.window.dispatch('hashchange')
  assert.deepEqual(selectedRows(app), [worktreeHashOf(pathB)])
})

const editSession = { ...session, org: 'o', repo: 'r', id: 's', dir: 'api/file/o/r/main/s/' }

// Waits until the pending fetches and renders are done.
const settle = () => new Promise(setImmediate)

// Returns a fake board API over the files of `editSession`. GET answers a file with its version as `ETag`. PUT checks `If-Match` and records the call.
function fileApi(files) {
  const versions = new Map(Object.keys(files).map((path) => [path, 1]))
  const puts = []
  const etag = (path) => `"v${versions.get(path)}"`

  // Changes `path` on disk to `text`.
  const touch = (path, text) => {
    files[path] = text
    versions.set(path, versions.get(path) + 1)
  }

  const fetcher = (url, options = {}) => {
    if (url === 'api/files/o/r/main/s') return filesReply(Object.keys(files))

    const path = url.slice(editSession.dir.length)
    if (!Object.hasOwn(files, path)) return response('', false, { status: 404 })
    if (options.method !== 'PUT') return response(files[path], true, { headers: { ETag: etag(path) } })

    const ifMatch = options.headers['If-Match']
    puts.push({ url, ifMatch, body: options.body })
    if (ifMatch !== etag(path)) return response('', false, { status: 409, headers: { ETag: etag(path) } })

    touch(path, options.body)
    return response('', true, { headers: { ETag: etag(path) } })
  }
  return { fetcher, files, puts, touch }
}

// Returns a dashboard that shows `path` of `editSession` from `api`.
async function openFile(api, path) {
  const app = dashboard({ fetcher: api.fetcher })
  app.context.session = editSession
  app.run('state.sessions = [session]')
  app.context.location.hash = app.run(`fileHref(session, ${JSON.stringify(path)})`)
  app.run('render()')
  await settle()
  return app
}

// Returns the editor field in the pane, or undefined when the pane holds none.
const editor = (app) => app.elements.get('view').children[0]

// Closes the open modal with the button of `value`.
function closeModal(app, value) {
  const modal = app.elements.get('modal')
  modal.returnValue = value
  modal.open = false
  modal.dispatch('close')
}

test('w opens the editor on markdown', async () => {
  const app = await openFile(fileApi({ 'spec.md': '# Spec\n' }), 'spec.md')

  assert.ok(press(app, 'w'))
  assert.equal(app.run('editing.path'), 'spec.md')
  assert.equal(app.run('editing.editor.value()'), '# Spec\n')
  assert.equal(editor(app).tagName, 'TEXTAREA')
  assert.equal(editor(app).focused, true)
})

test('w ignores files that are not markdown', async () => {
  for (const path of ['runs.jsonl', 'session.json']) {
    const app = await openFile(fileApi({ [path]: '{}\n' }), path)
    assert.match(app.elements.get('view').innerHTML, /\{\}/)

    press(app, 'w')
    assert.equal(app.run('editing'), null)
    assert.equal(editor(app), undefined)
  }
})

test('w while a new file loads leaves the editor closed', async () => {
  const api = fileApi({ 'spec.md': '# Spec\n', 'plan.md': '# Plan\n' })
  let release = null
  const held = new Promise((resolve) => { release = resolve })
  const fetcher = (url, options) => (url.endsWith('/plan.md') ? held.then(() => api.fetcher(url, options)) : api.fetcher(url, options))
  const app = await openFile({ fetcher }, 'spec.md')

  app.context.location.hash = app.run('fileHref(session, "plan.md")')
  app.window.dispatch('hashchange')
  await settle()
  assert.match(app.elements.get('viewer-head').innerHTML, /plan\.md/)

  press(app, 'w')
  assert.equal(app.run('editing'), null)
  assert.equal(editor(app), undefined)

  release()
  await settle()
  assert.match(app.elements.get('view').innerHTML, /# Plan/)

  press(app, 'w')
  assert.equal(app.run('editing.path'), 'plan.md')
  assert.equal(app.run('editing.editor.value()'), '# Plan\n')
})

test('Escape in the editor without changes returns to the view without a PUT', async () => {
  const api = fileApi({ 'spec.md': '# Spec\n' })
  const app = await openFile(api, 'spec.md')
  const href = app.context.location.hash

  press(app, 'w')
  assert.ok(press(app, 'Escape', {}, editor(app)))
  await settle()

  assert.deepEqual(api.puts, [])
  assert.equal(app.run('editing'), null)
  assert.equal(editor(app), undefined)
  assert.match(app.elements.get('view').innerHTML, /# Spec/)
  assert.equal(app.context.location.hash, href)
})

test('a CRLF file without typing leaves the editor without a PUT or a discard prompt', async () => {
  const api = fileApi({ 'spec.md': '# Spec\r\nBody\r\n' })
  const app = await openFile(api, 'spec.md')

  press(app, 'w')
  assert.equal(app.run('hasUnsavedChanges()'), false)
  press(app, 'Escape', {}, editor(app))
  await settle()

  assert.deepEqual(api.puts, [])
  assert.equal(app.run('editing'), null)

  press(app, 'w')
  assert.equal(app.run('editing.path'), 'spec.md')
  assert.ok(press(app, 'l'))
  await settle()

  assert.equal(app.elements.get('modal').open, undefined)
  assert.equal(app.run('editing'), null)
  assert.equal(app.context.location.hash, '#/sessions')
  assert.deepEqual(api.puts, [])
})

test('Escape in the editor with changes puts them with If-Match, then shows the new text', async () => {
  const api = fileApi({ 'spec.md': '# Spec\n' })
  const app = await openFile(api, 'spec.md')

  press(app, 'w')
  editor(app).value = '# Edited\n'
  press(app, 'Escape', {}, editor(app))
  await settle()

  assert.deepEqual(api.puts, [{ url: 'api/file/o/r/main/s/spec.md', ifMatch: '"v1"', body: '# Edited\n' }])
  assert.equal(api.files['spec.md'], '# Edited\n')
  assert.equal(app.run('editing'), null)
  assert.match(app.elements.get('view').innerHTML, /# Edited/)
})

test('Cmd+S in the editor saves and stays in edit mode', async () => {
  const api = fileApi({ 'spec.md': '# Spec\n' })
  const app = await openFile(api, 'spec.md')

  press(app, 'w')
  editor(app).value = '# Edited\n'
  assert.ok(press(app, 's', { metaKey: true }, editor(app)))
  await settle()

  assert.equal(api.puts.length, 1)
  assert.equal(api.files['spec.md'], '# Edited\n')
  assert.equal(app.run('editing.version'), 'v2')
  assert.equal(app.run('editing.original'), '# Edited\n')
  assert.equal(editor(app).value, '# Edited\n')
})

test('v outside the editor does not leave edit mode', async () => {
  const api = fileApi({ 'spec.md': '# Spec\n' })
  const app = await openFile(api, 'spec.md')

  press(app, 'w')
  const field = editor(app)
  field.value = '# Edited\n'
  assert.equal(press(app, 'v'), false)
  await settle()

  assert.deepEqual(api.puts, [])
  assert.equal(app.run('editing.path'), 'spec.md')
  assert.equal(editor(app), field)
  assert.equal(app.run('editing.editor.value()'), '# Edited\n')
})

test('Escape outside the editor returns to the list', async () => {
  const app = await openFile(fileApi({ 'spec.md': '# Spec\n' }), 'spec.md')

  press(app, 'w')
  assert.ok(press(app, 'Escape'))

  assert.equal(app.run('editing'), null)
  assert.equal(app.context.location.hash, '#/sessions')
})

test('a conflict keeps edit mode and the text, and shows a message in the viewer head', async () => {
  const api = fileApi({ 'spec.md': '# Spec\n' })
  const app = await openFile(api, 'spec.md')

  press(app, 'w')
  editor(app).value = '# Edited\n'
  api.touch('spec.md', '# Changed on disk\n')
  press(app, 'Escape', {}, editor(app))
  await settle()

  assert.equal(api.puts.length, 1)
  assert.equal(api.files['spec.md'], '# Changed on disk\n')
  assert.equal(app.run('editing.path'), 'spec.md')
  assert.equal(app.run('editing.editor.value()'), '# Edited\n')
  assert.equal(editor(app).value, '# Edited\n')
  assert.match(
    app.elements.get('viewer-head').innerHTML,
    /File changed on disk\. Copy your text, click outside the editor, leave the file and discard, then reopen it and press w\./,
  )
})

test('a poll while editing does not replace the editor', async () => {
  const api = fileApi({ 'spec.md': '# Spec\n' })
  const app = await openFile(api, 'spec.md')

  press(app, 'w')
  const field = editor(app)
  field.value = '# Typed\n'
  api.touch('spec.md', '# Changed on disk\n')
  app.run('render()')
  await settle()

  assert.equal(editor(app), field)
  assert.equal(app.run('editing.editor.value()'), '# Typed\n')
  assert.doesNotMatch(app.elements.get('view').innerHTML, /Changed on disk/)
})

test('leaving with changes asks to discard them', async () => {
  const api = fileApi({ 'spec.md': '# Spec\n' })
  const app = await openFile(api, 'spec.md')
  const href = app.context.location.hash

  press(app, 'w')
  editor(app).value = '# Typed\n'
  assert.ok(press(app, 'l'))
  assert.equal(app.elements.get('modal').open, true)
  assert.equal(app.elements.get('modal-text').textContent, 'Discard your changes?')

  closeModal(app, 'cancel')
  await settle()
  assert.equal(app.run('editing.editor.value()'), '# Typed\n')
  assert.equal(app.context.location.hash, href)

  press(app, 'l')
  closeModal(app, 'ok')
  await settle()
  assert.equal(app.run('editing'), null)
  assert.equal(app.context.location.hash, '#/sessions')
  assert.deepEqual(api.puts, [])
})

test('a route change with changes asks to discard them and restores the route on cancel', async () => {
  const app = await openFile(fileApi({ 'spec.md': '# Spec\n' }), 'spec.md')
  const href = app.context.location.hash

  press(app, 'w')
  const field = editor(app)
  field.value = '# Typed\n'
  app.context.location.hash = '#/'
  app.window.dispatch('hashchange')
  assert.equal(app.elements.get('modal').open, true)

  app.run('render()')
  await settle()
  assert.match(app.elements.get('page').innerHTML, /class="session-page"/, 'a poll keeps the session page while the modal is open')
  assert.equal(editor(app), field)

  closeModal(app, 'cancel')
  await settle()
  assert.equal(app.context.location.hash, href)
  assert.equal(app.run('editing.editor.value()'), '# Typed\n')
})

test('a stage change while editing the default file keeps the edited file in the pane', async () => {
  const app = dashboard({ fetcher: fileApi({ 'spec.md': '# Spec\n', 'plan.md': '# Plan\n' }).fetcher })
  app.context.session = { ...editSession, stage: 'spec' }
  app.run('state.sessions = [session]')
  app.context.location.hash = app.run('sessionHref(session)')
  app.run('render()')
  await settle()

  press(app, 'w')
  const field = editor(app)
  app.run('state.sessions = [{ ...session, stage: "plan" }]')
  app.run('render()')
  await settle()

  assert.equal(editor(app), field)
  assert.equal(app.run('editing.path'), 'spec.md')
  assert.match(app.elements.get('viewer-head').innerHTML, /spec\.md/)
  assert.doesNotMatch(app.elements.get('viewer-head').innerHTML, /plan\.md/)
})

// Gives `app` a fake app window API and returns the count of its close calls.
function appWindow(app) {
  const closes = { count: 0 }
  app.window.__TAURI__ = { window: { getCurrentWindow: () => ({ close: async () => { closes.count += 1 } }) } }
  return closes
}

test('closing the app window without changes closes it at once', async () => {
  const app = await openFile(fileApi({ 'spec.md': '# Spec\n' }), 'spec.md')
  const closes = appWindow(app)

  assert.equal(app.run('onCloseRequested()'), true)

  press(app, 'w')
  assert.equal(app.run('onCloseRequested()'), true)
  assert.equal(app.elements.get('modal').open, undefined)
  assert.equal(closes.count, 0)
})

test('closing the app window with changes asks to discard them, and closes it on discard', async () => {
  const api = fileApi({ 'spec.md': '# Spec\n' })
  const app = await openFile(api, 'spec.md')
  const closes = appWindow(app)

  press(app, 'w')
  editor(app).value = '# Typed\n'
  assert.equal(app.run('onCloseRequested()'), false)
  assert.equal(app.elements.get('modal').open, true)
  assert.equal(app.elements.get('modal-text').textContent, 'Discard your changes?')
  assert.equal(app.run('onCloseRequested()'), false, 'a second request while the modal is open')

  closeModal(app, 'cancel')
  await settle()
  assert.equal(closes.count, 0)
  assert.equal(app.run('editing.editor.value()'), '# Typed\n')

  assert.equal(app.run('onCloseRequested()'), false)
  closeModal(app, 'ok')
  await settle()
  assert.equal(closes.count, 1)
  assert.equal(app.run('editing'), null)
  assert.equal(app.run('onCloseRequested()'), true)
  assert.deepEqual(api.puts, [])
})

// Dispatches `beforeunload` to `app` and returns true when the page asks to stay.
function unload(app) {
  let prevented = false
  app.window.dispatch('beforeunload', { preventDefault: () => { prevented = true } })
  return prevented
}

test('a reload or page close asks to stay only while the editor holds unsaved changes', async () => {
  const app = await openFile(fileApi({ 'spec.md': '# Spec\n' }), 'spec.md')
  assert.equal(unload(app), false)

  press(app, 'w')
  assert.equal(unload(app), false)

  editor(app).value = '# Typed\n'
  assert.equal(unload(app), true)

  press(app, 's', { metaKey: true }, editor(app))
  await settle()
  assert.equal(unload(app), false)
})

// Returns a fake diff block of index 0, its body, and its Unified and Split buttons.
function diffBlock() {
  const body = {}
  const block = {
    dataset: { diff: '0' },
    querySelector: (selector) => (selector === '.diff-body' ? body : null),
    querySelectorAll: () => buttons,
  }
  const button = (layout, pressed) => {
    const el = {
      dataset: { layout },
      attributes: { 'aria-pressed': pressed },
      setAttribute: (name, value) => { el.attributes[name] = value },
      closest: (selector) => (selector.includes('button') ? el : selector.startsWith('.diff[') ? block : null),
    }
    return el
  }
  const buttons = [button('line-by-line', 'true'), button('side-by-side', 'false')]
  return { block, body, buttons }
}

test('a shown diff draws through diff2html and redraws in the clicked layout', async () => {
  const app = markdownDashboard({ fetcher: fileApi({ 'steps/01.md': diffFence }).fetcher })
  const draws = []
  app.context.Diff2HtmlUI = class {
    constructor(body, text, config, hljs) { this.call = { body, text, config, hljs } }
    draw() { draws.push(this.call) }
  }

  app.context.session = editSession
  app.run('state.sessions = [session]')
  app.context.location.hash = app.run('fileHref(session, "steps/01.md")')
  app.run('render()')

  const view = app.elements.get('view')
  const { block, body, buttons } = diffBlock()
  view.querySelectorAll = (selector) => (selector === '.diff[data-diff]' ? [block] : [])
  await settle()

  assert.ok(view.innerHTML.includes('<div class="diff" data-diff="0">'), view.innerHTML)
  assert.equal(draws.length, 1)
  assert.equal(draws[0].body, body)
  assert.equal(draws[0].text, diffText)
  assert.deepEqual(plain(draws[0].config), { outputFormat: 'line-by-line', drawFileList: false, matching: 'lines', highlight: true })
  assert.equal(draws[0].hljs, app.context.hljs)

  app.run('pendingDiffs = []')
  view.dispatch('click', { target: buttons[1] })
  assert.equal(draws.length, 2)
  assert.equal(draws[1].text, diffText)
  assert.equal(draws[1].config.outputFormat, 'side-by-side')
  assert.deepEqual(buttons.map((button) => button.attributes['aria-pressed']), ['false', 'true'])
})

// Returns a walk session of `editSession` at `stage`, with `done` of `planned` steps written.
const walkSession = (stage, done = 1, planned = 2, fields = {}) => ({
  ...editSession,
  flow: 'walk',
  stage,
  counts: { stepsPlanned: planned, stepsDone: done },
  blocked: null,
  ...fields,
})

const walkJson = JSON.stringify({
  mode: 'changes',
  summary: 'Adds rate limiting.',
  steps: [
    { n: 1, title: 'A', summary: 'The limiter.' },
    { n: 2, title: 'B', summary: 'The wiring.' },
  ],
})

// Returns the stops of `session` over `files` and `walkJson`.
function walkStops(app, session, files) {
  app.context.session = session
  app.context.files = files
  app.context.walkText = walkJson
  return plain(app.run('buildStops(session, files, [], {}, parseWalk(walkText))'))
}

// Returns a dashboard that shows the session page of `session` from `api`.
async function openSession(api, session) {
  const app = dashboard({ fetcher: api.fetcher })
  app.context.session = session
  app.run('state.sessions = [session]')
  app.context.location.hash = app.run('sessionHref(session)')
  app.run('render()')
  await settle()
  return app
}

test('classes sessions by flow', () => {
  const app = dashboard()
  const classOf = (flow) => {
    app.context.session = { ...session, flow }
    return app.run('sessionClass(session)')
  }

  assert.equal(classOf('build'), 'implementation')
  assert.equal(classOf('vibe'), 'implementation')
  assert.equal(classOf('review'), 'review')
  assert.equal(classOf('walk'), 'walk-through')
  assert.equal(classOf(undefined), 'implementation')
  assert.equal(classOf('other'), 'other')

  app.context.session = session
  assert.match(app.run('rowContent(session)'), /<span class="label"><span class="pill">implementation<\/span> /)
})

test('a row names its branch and run and shows the folder category', () => {
  const app = dashboard()
  app.context.session = { ...session, flow: 'walk', category: 'walk', id: 'walk-20261104-0912-3f00486', worktree: '/a/b/c/d' }
  const row = app.run('rowContent(session)')

  assert.match(row, /<span class="id">org\/repo\/main<\/span>/)
  assert.match(row, /<span class="path">walk-20261104-0912-3f00486 · b\/c\/d<\/span>/)
  assert.match(row, /<span class="pill">walk<\/span>/)

  app.context.session = { ...session, category: 'review' }
  assert.equal(app.run('sessionClass(session)'), 'review')
})

test('walk stops follow walk.json and the step files', () => {
  const app = dashboard()
  const stops = walkStops(app, walkSession('walk'), ['overview.md', 'walk.json', 'steps/01.md'])

  assert.deepEqual(stops.map((stop) => stop.label), ['Overview', 'Step 1: A', 'Step 2: B'])
  assert.deepEqual(stops.map((stop) => stop.file), ['overview.md', 'steps/01.md', null])
  assert.deepEqual(stops.map((stop) => stop.state), ['done', 'done', 'writing'])
  assert.deepEqual(stops.map((stop) => stop.summary), ['Adds rate limiting.', 'The limiter.', 'The wiring.'])

  app.context.stop = stops[2]
  const card = app.run('stopCardHtml(stop, null, false)')
  assert.match(card, /<span class="title">Step 2: B<\/span>/)
  assert.match(card, /<div class="card-foot"><span class="badge">writing<\/span><\/div>\s*<\/div>$/)
  assert.match(card, /<div class="note">The wiring\.<\/div>/)
  assert.doesNotMatch(card, /data-path=/)
  assert.match(html, /\.stop\.writing \{ --tone: var\(--accent\); \}/)
})

test('a done walk marks every written step done', () => {
  const app = dashboard()
  const done = walkSession('done', 2, 2)

  assert.deepEqual(walkStops(app, done, ['overview.md', 'steps/01.md', 'steps/02.md']).map((stop) => stop.state), ['done', 'done', 'done'])
  assert.deepEqual(walkStops(app, done, ['overview.md', 'steps/01.md']).map((stop) => stop.state), ['done', 'done', 'pending'])
  assert.equal(app.run('stepLabel(session)'), 'walk-through · done')
})

test('a blocked walk shows its reason on the overview', () => {
  const app = dashboard()
  const blocked = walkSession('blocked', 1, 2, { blocked: { stage: 'walk', reason: 'git failed' } })
  const stops = walkStops(app, blocked, ['overview.md', 'steps/01.md'])

  assert.deepEqual(stops.map((stop) => stop.state), ['blocked', 'done', 'pending'])
  assert.equal(stops[0].file, 'overview.md')
  assert.equal(stops[0].note, 'git failed')
  assert.deepEqual(stops[0].links, [{ label: 'session.json', path: 'session.json' }])
  assert.equal(app.run('stepLabel(session)'), 'walk-through · blocked')
  assert.equal(app.run('defaultFile(session, [], 0)'), 'session.json')
  assert.equal(app.run('stopIndexForFile(buildStops(session, files, [], {}, parseWalk(walkText)), "session.json")'), 0)
})

test('a walk without walk.json shows only the overview', async () => {
  const app = dashboard({ fetcher: () => response('', false) })
  for (const text of ['{', '{}', 'null', '7', '{"steps":{}}']) {
    assert.equal(app.run(`parseWalk(${JSON.stringify(text)})`), null, text)
  }

  app.context.walkText = JSON.stringify({
    summary: 7,
    steps: [{ n: 1, title: 'A', summary: 'S' }, { n: '2', title: 'B' }, { n: 3 }, null, { n: 4.5, title: 'C' }, { n: 5, title: 'E', summary: 1 }],
  })
  assert.deepEqual(plain(app.run('parseWalk(walkText)')), {
    summary: null,
    steps: [{ n: 1, title: 'A', summary: 'S' }, { n: 5, title: 'E', summary: null }],
  })

  assert.equal(await app.run('loadWalk("api/file/o/r/main/s/", ["session.json"])'), null)
  assert.deepEqual(app.calls, [])
  assert.equal(await app.run('loadWalk("api/file/o/r/main/s/", ["walk.json"])'), null)
  assert.deepEqual(app.calls, ['api/file/o/r/main/s/walk.json'])

  app.context.session = walkSession('walk', 0, 0)
  const stops = plain(app.run('buildStops(session, ["request.md"], [], {}, null)'))
  assert.equal(stops.length, 1)
  assert.equal(stops[0].label, 'Overview')
  assert.equal(stops[0].file, 'overview.md')
  assert.equal(stops[0].state, 'done')
  assert.equal(stops[0].summary, null)
})

test('walk rows read writing n/m', () => {
  const app = dashboard()
  app.context.session = walkSession('walk', 1, 2)

  assert.equal(app.run('stepLabel(session)'), 'walk-through · writing 1/2')
  assert.match(app.run('rowContent(session)'), /<span class="pill">walk-through<\/span> walk-through · writing 1\/2<\/span>/)

  const facts = [...app.run('sessionCardHtml(session, [])').matchAll(/<span class="pill">([^<]*)<\/span>/g)].map((match) => match[1])
  assert.deepEqual(facts, ['walk-through', 'walk', '1/2 steps'])

  app.context.session = walkSession('walk', 1, 2, { counts: undefined })
  assert.equal(app.run('stepLabel(session)'), 'walk-through · writing 0/0')
})

test('a walk opens its overview by default', async () => {
  const api = fileApi({ 'session.json': '{}', 'walk.json': walkJson, 'overview.md': '# Overview\n', 'steps/01.md': '## Step 1: A\n' })
  const app = await openSession(api, walkSession('walk'))

  assert.equal(app.run('defaultFile(session, [], 0)'), 'overview.md')
  assert.equal(app.run('sessionView.selected'), 0)
  assert.deepEqual(plain(app.run('sessionView.stops.map((stop) => stop.label)')), ['Overview', 'Step 1: A', 'Step 2: B'])
  assert.match(app.elements.get('viewer-head').innerHTML, /<span class="path">overview\.md<\/span><span class="by">Overview<\/span>/)
  assert.match(app.elements.get('view').innerHTML, /# Overview/)
})

test('moving between walk stops skips unwritten steps', async () => {
  const steps = [1, 2, 3].map((n) => ({ n, title: `S${n}` }))
  const api = fileApi({ 'walk.json': JSON.stringify({ steps }), 'overview.md': '# Overview\n', 'steps/01.md': '## 1\n', 'steps/03.md': '## 3\n' })
  const app = await openSession(api, walkSession('walk', 2, 3))
  const at = () => [app.run('sessionView.selected'), app.context.location.hash]
  const href = (path) => app.run(`fileHref(session, ${JSON.stringify(path)})`)

  assert.ok(press(app, 'ArrowDown'))
  assert.deepEqual(at(), [1, href('steps/01.md')])
  assert.ok(press(app, 'ArrowDown'))
  assert.deepEqual(at(), [3, href('steps/03.md')])
  assert.equal(press(app, 'ArrowDown'), false)
  assert.ok(press(app, 'ArrowUp'))
  assert.deepEqual(at(), [1, href('steps/01.md')])
  assert.ok(press(app, 'ArrowUp'))
  assert.deepEqual(at(), [0, href('overview.md')])
})

// Returns the bytes of a `Uint8Array` from any realm, and any other value unchanged.
const bytes = (value) => (Object.prototype.toString.call(value) === '[object Uint8Array]' ? Array.from(value) : value)

// Gives `app` fake xterm, fit addon, socket and resize observer classes, and returns the instances they create.
function terminalStubs(app) {
  const sockets = []
  const terminals = []
  const observers = []
  const { document } = app.context

  // Records what the page writes and focuses, and keeps the handlers it registers. `size` is what the next fit gives.
  class Terminal {
    cols = 80
    rows = 24
    size = { cols: 100, rows: 30 }
    written = []
    focused = false
    disposed = false
    fits = 0
    element = null
    handlers = { data: null, binary: null, resize: null, key: null }

    constructor(options) {
      this.options = options
      terminals.push(this)
    }

    loadAddon(addon) { addon.activate(this) }

    open(container) {
      this.element = document.createElement('div')
      container.replaceChildren(this.element)
    }

    write(data) { this.written.push(data) }
    focus() { this.focused = true }
    blur() { this.focused = false }
    dispose() { this.disposed = true }
    onData(handler) { this.handlers.data = handler }
    onBinary(handler) { this.handlers.binary = handler }
    onResize(handler) { this.handlers.resize = handler }
    attachCustomKeyEventHandler(handler) { this.handlers.key = handler }
  }

  // Sets its terminal to the terminal's `size`, and calls the resize handler when the size changed.
  class FitAddon {
    activate(terminal) { this.terminal = terminal }

    fit() {
      const terminal = this.terminal
      const { cols, rows } = terminal.size
      terminal.fits += 1
      if (terminal.cols === cols && terminal.rows === rows) return

      terminal.cols = cols
      terminal.rows = rows
      terminal.handlers.resize?.({ cols, rows })
    }
  }

  // Records its URL, binary type, sent frames and close. A test fires its handlers by hand.
  class WebSocket {
    static OPEN = 1
    readyState = 1
    binaryType = 'blob'
    sent = []
    closed = false

    constructor(url) {
      this.url = url
      sockets.push(this)
    }

    send(data) { this.sent.push(data) }

    close() {
      this.closed = true
      this.readyState = 3
    }
  }

  // Records its target and its callback. A test calls the callback by hand.
  class ResizeObserver {
    target = null
    disconnected = false

    constructor(callback) {
      this.callback = callback
      observers.push(this)
    }

    observe(target) { this.target = target }
    disconnect() { this.disconnected = true }
  }

  app.context.location.host = '127.0.0.1:8731'
  Object.assign(app.context, {
    Terminal,
    FitAddon: { FitAddon },
    WebSocket,
    ResizeObserver,
    TextEncoder,
  })
  return { sockets, terminals, observers }
}

// Returns a dashboard that shows `path` of `editSession` from `api`, with fake terminal classes, and the instances they create.
async function lazygitApp(api, path) {
  const app = await openFile(api, path)
  return { app, ...terminalStubs(app) }
}

// Returns a lazygit dashboard that shows `spec.md` of `editSession`.
const specLazygit = () => lazygitApp(fileApi({ 'spec.md': '# Spec\n' }), 'spec.md')

test('g on a session page runs lazygit in the pane', async () => {
  const { app, sockets, terminals } = await specLazygit()

  assert.ok(press(app, 'g'))
  assert.equal(sockets.length, 1)
  assert.equal(sockets[0].url, 'ws://127.0.0.1:8731/api/lazygit/o/r/main/s?cols=100&rows=30')
  assert.equal(sockets[0].binaryType, 'arraybuffer')

  const [xterm] = terminals
  assert.equal(xterm.focused, true)
  assert.deepEqual(app.elements.get('view').children, [xterm.element])
  assert.equal(xterm.options.fontFamily, 'ui-monospace, SFMono-Regular, Menlo, monospace')
  assert.equal(xterm.options.fontSize, 14)
  assert.equal(xterm.options.theme, app.run('TERMINAL_THEME'))
  assert.equal(xterm.options.theme.background, '#142b37')
  assert.equal(xterm.options.theme.foreground, '#e0e2e3')

  await settle()
  assert.match(app.elements.get('viewer-head').innerHTML, /<span class="path">lazygit<\/span>/)
  assert.doesNotMatch(app.elements.get('view').innerHTML, /# Spec/)
})

test('the lazygit card button runs lazygit', async () => {
  const { app, sockets } = await specLazygit()
  const actions = app.run('CARD_ACTIONS.map((entry) => `${entry.action}: ${entry.title}`).join(", ")')
  assert.equal(actions, 'intellij: Open in IntelliJ (i), zed: Open in Zed (e), iterm: Open in iTerm (t), lazygit: Open in lazygit (g), archive: Archive (a)')

  const button = { dataset: { action: 'lazygit' }, closest: () => ({ dataset: { dir: editSession.dir } }) }
  const target = { closest: (selector) => (selector === 'button[data-action]' ? button : null) }
  app.elements.get('session-card').dispatch('click', { target })
  assert.equal(sockets.length, 1)
})

test('g on the list opens the session page and lazygit', async () => {
  const { app, sockets, terminals } = await specLazygit()
  app.context.location.hash = '#/sessions'
  app.window.dispatch('hashchange')
  app.run('selectedDir = session.dir')

  assert.ok(press(app, 'g'))
  assert.equal(app.context.location.hash, app.run('sessionHref(session)'))
  assert.match(app.elements.get('page').innerHTML, /class="session-page"/)
  assert.equal(sockets.length, 1)
  assert.deepEqual(app.elements.get('view').children, [terminals[0].element])
})

test('bytes flow between the socket and the terminal', async () => {
  const { app, sockets, terminals } = await specLazygit()
  press(app, 'g')
  const [socket] = sockets
  const [xterm] = terminals

  socket.onmessage({ data: new Uint8Array([104, 105]).buffer })
  assert.deepEqual(xterm.written.map(bytes), [[104, 105]])

  xterm.handlers.data('q')
  xterm.handlers.binary('\x80')
  xterm.handlers.resize({ cols: 90, rows: 20 })
  assert.deepEqual(socket.sent.map(bytes), [[113], [128], '{"cols":90,"rows":20}'])

  socket.readyState = 0
  xterm.handlers.data('x')
  xterm.handlers.resize({ cols: 80, rows: 10 })
  assert.equal(socket.sent.length, 3, 'a socket that is not open gets no frames')
})

test('the socket sends the terminal size when it opens', async () => {
  const { app, sockets, terminals } = await specLazygit()
  press(app, 'g')

  sockets[0].readyState = 0
  terminals[0].size = { cols: 100, rows: 27 }
  app.run('terminal.fit.fit()')
  assert.deepEqual(sockets[0].sent, [])

  sockets[0].readyState = 1
  sockets[0].onopen()
  assert.deepEqual(sockets[0].sent, ['{"cols":100,"rows":27}'])
})

test('a pane resize refits the terminal and sends its size', async () => {
  const { app, sockets, terminals, observers } = await specLazygit()
  press(app, 'g')

  assert.equal(observers.length, 1)
  assert.equal(observers[0].target, app.elements.get('view'))

  terminals[0].size = { cols: 90, rows: 25 }
  observers[0].callback()
  assert.deepEqual(sockets[0].sent, ['{"cols":90,"rows":25}'])
})

test('keys inside the terminal fire no board shortcut', async () => {
  const { app, terminals } = await specLazygit()
  press(app, 'g')
  const href = app.context.location.hash

  assert.equal(press(app, 'l', {}, terminals[0].element), false)
  assert.equal(press(app, 'Escape', {}, terminals[0].element), false)
  assert.equal(app.context.location.hash, href)
})

test('a poll keeps lazygit', async () => {
  const { app, sockets, terminals } = await specLazygit()
  press(app, 'g')

  app.run('render()')
  await settle()
  assert.equal(sockets[0].closed, false)
  assert.equal(terminals[0].disposed, false)
  assert.deepEqual(app.elements.get('view').children, [terminals[0].element])
  assert.doesNotMatch(app.elements.get('view').innerHTML, /# Spec/)
})

test('lazygit exiting shows the file again', async () => {
  const { app, sockets, terminals } = await specLazygit()
  press(app, 'g')
  await settle()

  sockets[0].onclose({ code: 1000, reason: '' })
  await settle()
  assert.equal(app.run('terminal'), null)
  assert.equal(terminals[0].disposed, true)
  assert.match(app.elements.get('view').innerHTML, /# Spec/)
  assert.match(app.elements.get('viewer-head').innerHTML, /spec\.md/)
  assert.equal(app.elements.get('modal').open, undefined)
})

test('a failed lazygit shows the reason', async () => {
  const { app, sockets } = await specLazygit()
  press(app, 'g')

  sockets[0].onclose({ code: 1011, reason: 'session has no worktree' })
  assert.equal(app.run('terminal'), null)
  assert.equal(app.elements.get('modal').open, true)
  assert.equal(app.elements.get('modal-text').textContent, 'lazygit stopped for o/r/main/s: session has no worktree')

  closeModal(app, 'ok')
  await settle()
  press(app, 'g')
  sockets[1].onclose({ code: 1006, reason: '' })
  assert.equal(app.elements.get('modal-text').textContent, 'lazygit stopped for o/r/main/s: the connection closed')
})

test('a route change closes lazygit', async () => {
  const { app, sockets, terminals, observers } = await specLazygit()
  press(app, 'g')

  app.context.location.hash = '#/'
  app.window.dispatch('hashchange')
  assert.equal(sockets[0].closed, true)
  assert.equal(sockets[0].onclose, null)
  assert.equal(terminals[0].disposed, true)
  assert.equal(observers[0].disconnected, true)
  assert.equal(app.run('terminal'), null)
})

test('a session that disappears closes lazygit', async () => {
  const { app, sockets } = await specLazygit()
  press(app, 'g')

  app.run('state.sessions = []')
  app.run('render()')
  assert.equal(sockets[0].closed, true)
  assert.equal(app.run('terminal'), null)
  assert.match(app.elements.get('page').innerHTML, /session not found/)

  app.context.location.hash = '#/sessions'
  app.window.dispatch('hashchange')
  assert.match(app.elements.get('page').innerHTML, /No sessions yet\./)
})

test('picking a file closes lazygit', async () => {
  const { app, sockets } = await specLazygit()
  press(app, 'g')
  await settle()

  app.run('openPath(session, "spec.md")')
  assert.equal(sockets[0].closed, true)

  await settle()
  assert.match(app.elements.get('view').innerHTML, /# Spec/)
})

test('g with unsaved editor changes asks to discard first', async () => {
  const { app, sockets } = await specLazygit()

  press(app, 'w')
  editor(app).value = '# Typed\n'
  assert.ok(press(app, 'g'))
  assert.equal(app.elements.get('modal').open, true)
  assert.equal(app.elements.get('modal-text').textContent, 'Discard your changes?')
  assert.equal(sockets.length, 0)

  closeModal(app, 'cancel')
  await settle()
  assert.equal(sockets.length, 0)
  assert.equal(app.run('editing.editor.value()'), '# Typed\n')

  press(app, 'g')
  closeModal(app, 'ok')
  await settle()
  assert.equal(app.run('editing'), null)
  assert.equal(sockets.length, 1)
})

test('g while lazygit runs focuses it', async () => {
  const { app, sockets, terminals } = await specLazygit()
  press(app, 'g')

  terminals[0].blur()
  assert.ok(press(app, 'g'))
  assert.equal(sockets.length, 1)
  assert.equal(terminals.length, 1)
  assert.equal(terminals[0].focused, true)
})

test('a font size change resizes the terminal', async () => {
  const { app, sockets, terminals } = await specLazygit()
  press(app, 'g')
  const [xterm] = terminals
  const fits = xterm.fits

  xterm.size = { cols: 120, rows: 36 }
  app.run('stepFontSize(1)')
  assert.equal(xterm.options.fontSize, Math.round((18 + 1) * 0.8))
  assert.equal(xterm.fits, fits + 1)
  assert.deepEqual(sockets[0].sent, ['{"cols":120,"rows":36}'])
})

test('a late close from an old socket leaves the new lazygit open', async () => {
  const { app, sockets } = await specLazygit()
  press(app, 'g')
  const lateClose = sockets[0].onclose

  app.context.location.hash = '#/'
  app.window.dispatch('hashchange')
  app.run('openLazygit(session)')
  lateClose({ code: 1011, reason: '' })

  assert.equal(sockets.length, 2)
  assert.equal(sockets[1].closed, false)
  assert.equal(app.run('terminal.socket'), sockets[1])
  assert.equal(app.elements.get('modal').open, undefined)
})

const zoomKeys = { metaKey: true, shiftKey: true }

test('Shift+Cmd+K zooms lazygit in and Shift+Cmd+J zooms it out', async () => {
  for (const [zoomIn, zoomOut] of [['K', 'J'], ['k', 'j']]) {
    for (const inside of [true, false]) {
      const { app, terminals } = await specLazygit()
      press(app, 'g')
      const [xterm] = terminals
      const page = app.elements.get('session-page')
      const target = inside ? xterm.element : undefined
      const fits = xterm.fits

      xterm.blur()
      assert.ok(press(app, zoomIn, zoomKeys, target), `${zoomIn} is taken`)
      assert.equal(page.classList.contains('zoomed'), true)
      assert.equal(xterm.fits, fits + 1)
      assert.equal(xterm.focused, true)

      xterm.blur()
      assert.ok(press(app, zoomOut, zoomKeys, target), `${zoomOut} is taken`)
      assert.equal(page.classList.contains('zoomed'), false)
      assert.equal(xterm.fits, fits + 2)
      assert.equal(xterm.focused, true)
    }
  }
})

test('the zoom keys do nothing without lazygit', async () => {
  const { app } = await specLazygit()
  const page = app.elements.get('session-page')

  assert.equal(press(app, 'K', zoomKeys), false)
  assert.equal(page.classList.contains('zoomed'), false)
  assert.equal(press(app, 'J', zoomKeys), false)

  app.run('zoomTerminal(true)')
  assert.equal(page.classList.contains('zoomed'), false)
})

test('lazygit exiting leaves the zoom', async () => {
  const { app, sockets } = await specLazygit()
  press(app, 'g')
  press(app, 'K', zoomKeys)
  const page = app.elements.get('session-page')
  assert.equal(page.classList.contains('zoomed'), true)

  sockets[0].onclose({ code: 1000, reason: '' })
  await settle()
  assert.equal(page.classList.contains('zoomed'), false)
  assert.match(app.elements.get('view').innerHTML, /# Spec/)
})

test('the terminal keeps the zoom keys from lazygit', async () => {
  const { app, terminals } = await specLazygit()
  press(app, 'g')
  const { key } = terminals[0].handlers
  const event = (name, modifiers = {}) => ({ key: name, metaKey: false, ctrlKey: false, altKey: false, shiftKey: false, ...modifiers })

  for (const name of ['K', 'k', 'J', 'j']) assert.equal(key(event(name, zoomKeys)), false, `${name} stays out of lazygit`)
  assert.equal(key(event('k')), true)
  assert.equal(key(event('k', { metaKey: true })), true)
})

const mainPath = '/s/o/r/main'
const worktreePath = "/s/o/r/work tree's"
const encodedWorktree = "%2Fs%2Fo%2Fr%2Fwork%20tree's"
const worktreeRepos = {
  error: null,
  repos: [
    {
      org: 'o',
      repo: 'r',
      path: mainPath,
      error: null,
      worktrees: [
        { path: mainPath, branch: 'main', head: null },
        { path: worktreePath, branch: 'feature', head: null },
      ],
    },
  ],
}

// Returns the hash of the worktree page of `path`.
const worktreeHashOf = (path) => `#/worktree/${encodeURIComponent(path)}`

// Returns a dashboard on the worktree page of `path` after one poll of an api that lists `sessions` and answers `api/repos` with `reposReply()`, with fake terminal classes, the instances they create and the URLs it posts.
async function worktreeApp(path = worktreePath, sessions = [], reposReply = () => response(JSON.stringify(worktreeRepos))) {
  const posts = []
  const app = dashboard({
    fetcher: (url, options) => {
      if (options?.method === 'POST') {
        posts.push(url)
        return response('', true, { status: 204 })
      }
      if (url === 'api/repos') return reposReply()
      if (url === 'api/sessions') return sessionsReply(sessions.map(sessionEntry))
      return response('', false, { status: 404 })
    },
  })
  const stubs = terminalStubs(app)
  app.context.location.hash = worktreeHashOf(path)
  await app.run('poll()')
  return { app, posts, ...stubs }
}

// Goes to `hash` as a click on a link does.
function navigate(app, hash) {
  app.context.location.hash = hash
  app.window.dispatch('hashchange')
}

// Clicks the tab strip button of `action` for the tab `id`.
function clickTab(app, action, id) {
  const button = { dataset: { term: action, tab: String(id) } }
  const target = { closest: (selector) => (selector === 'button[data-term]' ? button : null) }
  app.elements.get('term-tabs').dispatch('click', { target })
}

// Clicks the worktree card button of `action`.
function clickCard(app, action) {
  const button = { dataset: { action } }
  const target = { closest: (selector) => (selector === 'button[data-action]' ? button : null) }
  app.elements.get('worktree-card').dispatch('click', { target })
}

// Returns the terminal tabs of the worktree at `path`.
const tabsOf = (app, path = worktreePath) => Array.from(app.run(`worktreeTerms.get(${JSON.stringify(path)})?.tabs ?? []`))

// Returns the names in the tab strip, with `*` after the selected tab and `!` after a failed one.
const tabNames = (app) => [...app.elements.get('term-tabs').innerHTML.matchAll(/<span class="term-tab( failed)?">\s*<button type="button" class="term-name" data-term="select" data-tab="\d+" aria-pressed="(true|false)">([^<]*)<\/button>/g)]
  .map((match) => `${match[3]}${match[2] === 'true' ? '*' : ''}${match[1] ? '!' : ''}`)

// Returns the elements in the terminal pane of the worktree page.
const shownTerms = (app) => app.elements.get('term-view').children

test('the worktree page lists its sessions as links', async () => {
  const at = (id, worktree, fields = {}) => ({ ...session, org: 'o', repo: 'r', id, dir: `api/file/o/r/main/${id}/`, worktree, ...fields })
  const sessions = [at('s1', worktreePath), at('s2', `${worktreePath}/`, { flow: 'review', stage: 'verify' }), at('s3', mainPath)]
  const { app } = await worktreeApp(worktreePath, sessions)

  // Returns the session links of the left pane, each with its decoded link and its text.
  const links = () => [...app.elements.get('worktree-sessions').innerHTML.matchAll(/<a class="body" href="([^"]*)">([\s\S]*?)<\/a>/g)]
    .map((match) => ({ href: unescapeHtml(match[1]), text: textOf(match[2]) }))

  assert.deepEqual(links(), [
    { href: '#/o/r/main/s1', text: 's1 build implement' },
    { href: '#/o/r/main/s2', text: 's2 review verify' },
  ])

  const card = app.elements.get('worktree-card').innerHTML
  assert.equal(textOf(card), "o/r o/r/work tree's feature")
  assert.deepEqual([...card.matchAll(/data-action="([^"]*)"/g)].map((match) => match[1]), ['lazygit', 'iterm', 'zed', 'intellij', 'github'])
  assert.match(app.elements.get('page').innerHTML, /<div class="session-bar" id="worktree-card"><\/div>[\s\S]*<div id="splitter"><\/div>/)
  assert.match(app.elements.get('term-tabs').innerHTML, /<button type="button" class="term-add" data-term="add" title="[^"]*">\+<\/button>/)

  navigate(app, worktreeHashOf(mainPath))
  assert.deepEqual(links(), [{ href: '#/o/r/main/s3', text: 's3 build implement' }])
})

test('the worktree buttons show their keys', async () => {
  const { app } = await worktreeApp()

  const card = app.elements.get('worktree-card').innerHTML
  assert.deepEqual([...card.matchAll(/data-action="([^"]*)" title="([^"]*)"/g)].map((match) => `${match[1]}: ${match[2]}`), [
    'lazygit: Open lazygit in a tab (g)',
    'iterm: Open in iTerm',
    'zed: Open in Zed (e)',
    'intellij: Open in IntelliJ (i)',
    'github: Open on GitHub (o)',
  ])
  assert.match(app.elements.get('term-tabs').innerHTML, /<button type="button" class="term-add" data-term="add" title="Open a shell \(t\)">\+<\/button>/)
})

test('plus opens a shell tab on the terminal route', async () => {
  const { app, sockets, terminals } = await worktreeApp()

  clickTab(app, 'add')
  const [first] = tabsOf(app)
  assert.equal(sockets.length, 1)
  assert.equal(sockets[0].url, `ws://127.0.0.1:8731/api/terminal?path=${encodedWorktree}&program=shell&cols=100&rows=30`)
  assert.equal(sockets[0].binaryType, 'arraybuffer')
  assert.deepEqual(tabNames(app), ['shell 1*'])
  assert.deepEqual(shownTerms(app), [first.el])
  assert.equal(terminals[0].element.parentElement, first.el)
  assert.equal(terminals[0].focused, true)
  assert.equal(terminals[0].options.theme, app.run('TERMINAL_THEME'))

  clickTab(app, 'add')
  const [, second] = tabsOf(app)
  assert.equal(sockets[1].url, `ws://127.0.0.1:8731/api/terminal?path=${encodedWorktree}&program=shell&cols=100&rows=30`)
  assert.deepEqual(tabNames(app), ['shell 1', 'shell 2*'])
  assert.deepEqual(shownTerms(app), [second.el])
  assert.equal(first.el.parentElement, app.elements.get('term-store'))

  const view = app.elements.get('term-view')
  await app.run('poll()')
  assert.equal(app.elements.get('term-view'), view, 'a poll keeps the page')
  assert.deepEqual(shownTerms(app), [second.el])
})

test('the lazygit button opens a lazygit tab', async () => {
  const { app, sockets } = await worktreeApp()

  clickCard(app, 'lazygit')
  assert.equal(sockets.length, 1)
  assert.equal(sockets[0].url, `ws://127.0.0.1:8731/api/terminal?path=${encodedWorktree}&program=lazygit&cols=100&rows=30`)
  assert.deepEqual(tabNames(app), ['lazygit*'])

  clickTab(app, 'add')
  assert.deepEqual(tabNames(app), ['lazygit', 'shell 1*'])
})

test('closing a tab closes its socket', async () => {
  const { app, sockets, terminals, observers } = await worktreeApp()
  clickTab(app, 'add')
  clickTab(app, 'add')
  const [first, second] = tabsOf(app)

  clickTab(app, 'close', second.id)
  assert.equal(sockets[1].closed, true)
  assert.equal(sockets[1].onclose, null)
  assert.equal(terminals[1].disposed, true)
  assert.equal(observers[1].disconnected, true)
  assert.equal(second.el.parentElement, null)
  assert.equal(sockets[0].closed, false)
  assert.deepEqual(tabNames(app), ['shell 1*'])
  assert.deepEqual(shownTerms(app), [first.el])

  clickTab(app, 'close', first.id)
  assert.equal(sockets[0].closed, true)
  assert.deepEqual(tabNames(app), [])
  assert.deepEqual(shownTerms(app), [])

  clickTab(app, 'add')
  assert.deepEqual(tabNames(app), ['shell 3*'])
})

test('program exit closes its tab', async () => {
  const { app, sockets, terminals } = await worktreeApp()
  clickTab(app, 'add')
  clickTab(app, 'add')
  const [first] = tabsOf(app)

  sockets[1].onclose({ code: 1000, reason: '' })
  assert.equal(terminals[1].disposed, true)
  assert.deepEqual(tabNames(app), ['shell 1*'])
  assert.deepEqual(shownTerms(app), [first.el])
  assert.equal(app.elements.get('modal').open, undefined)
})

test('a failed start keeps the tab with the reason', async () => {
  const { app, sockets, terminals } = await worktreeApp()
  clickTab(app, 'add')
  const [first] = tabsOf(app)

  sockets[0].onclose({ code: 1011, reason: 'not a configured worktree' })
  assert.equal(first.failed, 'not a configured worktree')
  assert.equal(terminals[0].disposed, false)
  assert.ok(terminals[0].written.some((data) => String(data).includes('not a configured worktree')), terminals[0].written.join())
  assert.deepEqual(tabNames(app), ['shell 1*!'])
  assert.deepEqual(shownTerms(app), [first.el])
  assert.equal(app.elements.get('modal').open, undefined)

  clickTab(app, 'add')
  sockets[1].onclose({ code: 1006, reason: '' })
  assert.equal(tabsOf(app)[1].failed, 'the connection closed')

  clickTab(app, 'close', first.id)
  assert.deepEqual(tabNames(app), ['shell 2*!'])
})

test('terminals survive leaving and returning', async () => {
  const at = { ...session, org: 'o', repo: 'r', id: 's1', dir: 'api/file/o/r/main/s1/', worktree: worktreePath }
  const { app, sockets, terminals } = await worktreeApp(worktreePath, [at])
  clickTab(app, 'add')
  clickTab(app, 'add')
  const [first, second] = tabsOf(app)
  clickTab(app, 'select', first.id)
  assert.deepEqual(tabNames(app), ['shell 1*', 'shell 2'])

  navigate(app, '#/o/r/main/s1')
  await settle()
  assert.match(app.elements.get('page').innerHTML, /id="metro-pane"/)
  assert.deepEqual(new Set(app.elements.get('term-store').children), new Set([first.el, second.el]))

  app.run('render()')
  await settle()
  navigate(app, worktreeHashOf(worktreePath))
  assert.equal(terminals.length, 2)
  assert.deepEqual(tabsOf(app).map((tab) => tab.term), terminals)
  assert.deepEqual(sockets.map((socket) => socket.closed), [false, false])
  assert.deepEqual(terminals.map((terminal) => terminal.disposed), [false, false])
  assert.deepEqual(tabNames(app), ['shell 1*', 'shell 2'])
  assert.deepEqual(shownTerms(app), [first.el])
  assert.deepEqual(app.elements.get('term-store').children, [second.el])
})

test('each worktree keeps its own tabs', async () => {
  const { app, sockets } = await worktreeApp()
  clickTab(app, 'add')
  clickTab(app, 'add')
  const own = tabsOf(app)

  navigate(app, worktreeHashOf(mainPath))
  assert.deepEqual(tabNames(app), [])
  assert.deepEqual(shownTerms(app), [])

  clickTab(app, 'add')
  const [other] = tabsOf(app, mainPath)
  assert.equal(sockets[2].url, 'ws://127.0.0.1:8731/api/terminal?path=%2Fs%2Fo%2Fr%2Fmain&program=shell&cols=100&rows=30')
  assert.deepEqual(tabNames(app), ['shell 1*'])
  assert.deepEqual(shownTerms(app), [other.el])

  navigate(app, worktreeHashOf(worktreePath))
  assert.deepEqual(tabNames(app), ['shell 1', 'shell 2*'])
  assert.deepEqual(shownTerms(app), [own[1].el])
  assert.equal(other.el.parentElement, app.elements.get('term-store'))
  assert.deepEqual(sockets.map((socket) => socket.closed), [false, false, false])
})

test('the repos page counts open terminals', async () => {
  const { app, sockets } = await worktreeApp()
  clickTab(app, 'add')
  clickTab(app, 'add')

  navigate(app, '#/')
  assert.deepEqual(worktreeRows(app).map((row) => row.text), [
    'main o/r/main 0 sessions',
    "feature o/r/work tree's 0 sessions · ▣ 2",
  ])

  sockets[0].onclose({ code: 1000, reason: '' })
  assert.equal(worktreeRows(app)[1].text, "feature o/r/work tree's 0 sessions · ▣ 1")
})

test('card buttons post to the worktree open route', async () => {
  const { app, posts, sockets } = await worktreeApp()

  for (const tool of ['zed', 'iterm', 'intellij', 'github']) clickCard(app, tool)
  await settle()
  assert.deepEqual(posts, ['zed', 'iterm', 'intellij', 'github'].map((tool) => `api/open/${tool}?path=${encodedWorktree}`))
  assert.equal(sockets.length, 0)
  assert.equal(app.elements.get('modal').open, undefined)
})

test('an unknown worktree path says so', async () => {
  const { app } = await worktreeApp('/s/o/r')
  assert.equal(pageText(app), 'not a configured worktree')
  assert.doesNotMatch(app.elements.get('page').innerHTML, /term-tabs/)

  const loading = dashboard()
  loading.context.location.hash = worktreeHashOf(worktreePath)
  loading.run('render()')
  assert.equal(pageText(loading), 'loading…')
})

test('a worktree page without tabs shows the repos error', async () => {
  const { app } = await worktreeApp(worktreePath, [], () => response('', false, { status: 500 }))
  assert.equal(pageText(app), '500 api/repos')
  assert.match(app.elements.get('page').innerHTML, /class="config-error"/)
  assert.doesNotMatch(app.elements.get('page').innerHTML, /term-tabs/)
})

test('a config error keeps the terminal tabs and shows the error', async () => {
  const error = 'Cannot read the repos from /h/.jam-factory/config.json: expected value'
  let repos = worktreeRepos
  const { app, sockets, terminals } = await worktreeApp(worktreePath, [], () => response(JSON.stringify(repos)))
  clickTab(app, 'add')
  const [first] = tabsOf(app)
  const view = app.elements.get('term-view')
  const card = () => app.elements.get('worktree-card').innerHTML

  repos = { error, repos: [] }
  await app.run('poll()')
  assert.equal(app.elements.get('term-view'), view)
  assert.deepEqual(tabNames(app), ['shell 1*'])
  assert.deepEqual(shownTerms(app), [first.el])
  assert.equal(sockets[0].closed, false)
  assert.equal(textOf(card()), `/s/o/r/work tree's ${error}`)
  assert.match(card(), /class="config-error"/)
  assert.doesNotMatch(card(), /data-action/)

  repos = worktreeRepos
  await app.run('poll()')
  assert.equal(app.elements.get('term-view'), view)
  assert.deepEqual(shownTerms(app), [first.el])
  assert.equal(terminals[0].focused, true)
  assert.equal(sockets[0].closed, false)
  assert.equal(textOf(card()), "o/r o/r/work tree's feature")
})

test('a removed worktree keeps its open tabs reachable and closable', async () => {
  const [main] = worktreeRepos.repos[0].worktrees
  let repos = worktreeRepos
  const { app, sockets } = await worktreeApp(worktreePath, [], () => response(JSON.stringify(repos)))
  clickTab(app, 'add')
  clickTab(app, 'add')
  const [first, second] = tabsOf(app)

  repos = { error: null, repos: [{ ...worktreeRepos.repos[0], worktrees: [main] }] }
  await app.run('poll()')
  assert.equal(textOf(app.elements.get('worktree-card').innerHTML), "/s/o/r/work tree's not a configured worktree")
  assert.deepEqual(tabNames(app), ['shell 1', 'shell 2*'])
  assert.deepEqual(shownTerms(app), [second.el])

  navigate(app, '#/')
  navigate(app, worktreeHashOf(worktreePath))
  assert.deepEqual(tabNames(app), ['shell 1', 'shell 2*'])
  assert.deepEqual(shownTerms(app), [second.el])

  clickTab(app, 'select', first.id)
  assert.deepEqual(tabNames(app), ['shell 1*', 'shell 2'])
  assert.deepEqual(shownTerms(app), [first.el])

  clickTab(app, 'close', first.id)
  assert.equal(sockets[0].closed, true)
  assert.deepEqual(tabNames(app), ['shell 2*'])

  clickTab(app, 'close', second.id)
  assert.equal(sockets[1].closed, true)
  assert.equal(pageText(app), 'not a configured worktree')
  assert.doesNotMatch(app.elements.get('page').innerHTML, /term-tabs/)
})

test('keys inside a worktree terminal fire no board shortcut', async () => {
  const { app, terminals } = await worktreeApp()
  clickTab(app, 'add')
  const hash = app.context.location.hash

  assert.equal(press(app, 'l', {}, terminals[0].element), false)
  assert.equal(press(app, 'Escape', {}, terminals[0].element), false)
  assert.equal(app.context.location.hash, hash)

  assert.ok(press(app, 'Escape'))
  assert.equal(app.context.location.hash, '#/')
})

test('a font size change resizes the worktree terminals', async () => {
  const { app, sockets, terminals } = await worktreeApp()
  clickTab(app, 'add')

  terminals[0].size = { cols: 120, rows: 36 }
  app.run('stepFontSize(1)')
  assert.equal(terminals[0].options.fontSize, Math.round((18 + 1) * 0.8))
  assert.deepEqual(sockets[0].sent, ['{"cols":120,"rows":36}'])
})

test('Shift+Cmd+K zooms the worktree terminal in and Shift+Cmd+J zooms it out', async () => {
  for (const [zoomIn, zoomOut] of [['K', 'J'], ['k', 'j']]) {
    for (const inside of [true, false]) {
      const { app, terminals } = await worktreeApp()
      clickTab(app, 'add')
      const [xterm] = terminals
      const page = app.elements.get('session-page')
      const target = inside ? xterm.element : undefined
      const fits = xterm.fits

      xterm.blur()
      assert.ok(press(app, zoomIn, zoomKeys, target), `${zoomIn} is taken`)
      assert.equal(page.classList.contains('zoomed'), true)
      assert.equal(xterm.fits, fits + 1)
      assert.equal(xterm.focused, true)

      xterm.blur()
      assert.ok(press(app, zoomOut, zoomKeys, target), `${zoomOut} is taken`)
      assert.equal(page.classList.contains('zoomed'), false)
      assert.equal(xterm.fits, fits + 2)
      assert.equal(xterm.focused, true)
    }
  }
})

test('the zoom keys do nothing on a worktree page without tabs', async () => {
  const { app } = await worktreeApp()
  const page = app.elements.get('session-page')

  assert.equal(press(app, 'K', zoomKeys), false)
  assert.equal(page.classList.contains('zoomed'), false)
})

test('closing a tab keeps the zoom until the last one', async () => {
  const { app } = await worktreeApp()
  clickTab(app, 'add')
  clickTab(app, 'add')
  const [first, second] = tabsOf(app)
  const page = app.elements.get('session-page')
  assert.ok(press(app, 'K', zoomKeys))

  clickTab(app, 'close', second.id)
  assert.equal(page.classList.contains('zoomed'), true)
  assert.deepEqual(shownTerms(app), [first.el])

  clickTab(app, 'close', first.id)
  assert.equal(page.classList.contains('zoomed'), false)
})

test('worktree terminals keep the zoom keys', async () => {
  const { app, terminals } = await worktreeApp()
  clickTab(app, 'add')
  const { key } = terminals[0].handlers
  const event = (name, modifiers = {}) => ({ key: name, metaKey: false, ctrlKey: false, altKey: false, shiftKey: false, ...modifiers })

  for (const name of ['K', 'k', 'J', 'j']) assert.equal(key(event(name, zoomKeys)), false, `${name} stays out of the terminal`)
  assert.equal(key(event('k')), true)
})

test('leaving the worktree page drops the zoom', async () => {
  const { app } = await worktreeApp()
  clickTab(app, 'add')
  assert.ok(press(app, 'K', zoomKeys))
  assert.equal(app.elements.get('session-page').classList.contains('zoomed'), true)

  navigate(app, '#/')
  navigate(app, worktreeHashOf(worktreePath))
  assert.equal(app.elements.get('session-page').classList.contains('zoomed'), false)
})

test('the zoom keys leave a parked worktree terminal alone on another page', async () => {
  const { app, terminals } = await worktreeApp()
  clickTab(app, 'add')
  navigate(app, '#/')
  terminals[0].blur()

  assert.equal(press(app, 'K', zoomKeys), false)
  assert.equal(terminals[0].focused, false)
})

const tabKeyModifiers = { metaKey: true, shiftKey: true }
const cmd = { metaKey: true }

// Presses `key` in the tab name field and returns true when the page took it.
function pressInRename(app, key) {
  let prevented = false
  app.elements.get('term-tabs').dispatch('keydown', {
    key,
    target: app.elements.get('term-rename'),
    stopPropagation: () => {},
    preventDefault: () => { prevented = true },
  })
  return prevented
}

test('Shift+Cmd+[ and Shift+Cmd+] move through the tabs and wrap around', async () => {
  const { app, terminals } = await worktreeApp()
  for (let i = 0; i < 3; i += 1) clickTab(app, 'add')
  const tabs = tabsOf(app)
  assert.deepEqual(tabNames(app), ['shell 1', 'shell 2', 'shell 3*'])

  assert.ok(press(app, '}', tabKeyModifiers, terminals[2].element))
  assert.deepEqual(tabNames(app), ['shell 1*', 'shell 2', 'shell 3'])
  assert.deepEqual(shownTerms(app), [tabs[0].el])
  assert.equal(terminals[0].focused, true)

  assert.ok(press(app, '{', tabKeyModifiers))
  assert.deepEqual(tabNames(app), ['shell 1', 'shell 2', 'shell 3*'])

  assert.ok(press(app, '[', tabKeyModifiers))
  assert.deepEqual(tabNames(app), ['shell 1', 'shell 2*', 'shell 3'])

  assert.ok(press(app, ']', tabKeyModifiers))
  assert.deepEqual(tabNames(app), ['shell 1', 'shell 2', 'shell 3*'])
})

test('Cmd+T opens a shell tab', async () => {
  const { app, sockets, terminals } = await worktreeApp()

  assert.ok(press(app, 't', cmd))
  assert.equal(sockets[0].url, `ws://127.0.0.1:8731/api/terminal?path=${encodedWorktree}&program=shell&cols=100&rows=30`)
  assert.deepEqual(tabNames(app), ['shell 1*'])

  assert.ok(press(app, 't', cmd, terminals[0].element))
  assert.deepEqual(tabNames(app), ['shell 1', 'shell 2*'])
})

test('the tab keys do nothing without tabs or off the worktree page', async () => {
  const { app, sockets } = await worktreeApp()

  assert.equal(press(app, ']', tabKeyModifiers), false)
  assert.equal(press(app, 'i', cmd), false)
  assert.equal(app.elements.get('term-rename'), undefined)

  navigate(app, '#/')
  assert.equal(press(app, 't', cmd), false)
  assert.equal(sockets.length, 0)
})

test('worktree terminals keep the tab keys', async () => {
  const { app, terminals } = await worktreeApp()
  clickTab(app, 'add')
  const { key } = terminals[0].handlers
  const event = (name, modifiers = {}) => ({ key: name, metaKey: false, ctrlKey: false, altKey: false, shiftKey: false, ...modifiers })

  for (const name of ['[', '{', ']', '}']) assert.equal(key(event(name, tabKeyModifiers)), false, `${name} stays out of the terminal`)
  for (const name of ['i', 't']) assert.equal(key(event(name, cmd)), false, `${name} stays out of the terminal`)
  assert.equal(key(event('i')), true)
  assert.equal(key(event('[', cmd)), true)
})

test('Cmd+I edits the active tab name and Enter saves it', async () => {
  const { app, terminals } = await worktreeApp()
  clickTab(app, 'add')
  clickTab(app, 'add')

  assert.ok(press(app, 'i', cmd, terminals[1].element))
  const input = app.elements.get('term-rename')
  assert.equal(input.value, 'shell 2')
  assert.equal(input.focused, true)
  assert.equal(input.selected, true)
  assert.deepEqual(tabNames(app), ['shell 1'])

  assert.equal(press(app, '}', tabKeyModifiers), false, 'the tab keys wait for the edit')
  assert.equal(press(app, 'i', cmd), false)

  terminals[1].blur()
  input.value = '  build  '
  assert.ok(pressInRename(app, 'Enter'))
  assert.deepEqual(tabNames(app), ['shell 1', 'build*'])
  assert.equal(app.elements.get('term-rename'), undefined)
  assert.equal(terminals[1].focused, true)
})

test('Escape drops the edit, and a blank name keeps the old one', async () => {
  const { app } = await worktreeApp()
  clickTab(app, 'add')

  press(app, 'i', cmd)
  app.elements.get('term-rename').value = 'build'
  assert.ok(pressInRename(app, 'Escape'))
  assert.deepEqual(tabNames(app), ['shell 1*'])
  assert.equal(app.context.location.hash, worktreeHashOf(worktreePath))

  press(app, 'i', cmd)
  app.elements.get('term-rename').value = '   '
  pressInRename(app, 'Enter')
  assert.deepEqual(tabNames(app), ['shell 1*'])
})

test('leaving the text field saves the name', async () => {
  const { app } = await worktreeApp()
  clickTab(app, 'add')

  press(app, 'i', cmd)
  const input = app.elements.get('term-rename')
  input.value = 'build'
  app.elements.get('term-tabs').dispatch('focusout', { target: input })
  assert.deepEqual(tabNames(app), ['build*'])
})

test('a poll keeps the text field and its draft', async () => {
  const { app } = await worktreeApp()
  clickTab(app, 'add')

  press(app, 'i', cmd)
  const input = app.elements.get('term-rename')
  input.value = 'bui'
  await app.run('poll()')
  assert.equal(app.elements.get('term-rename'), input)
  assert.equal(input.value, 'bui')
})

test('the edit ends when its tab closes or the page changes', async () => {
  const { app, sockets } = await worktreeApp()
  clickTab(app, 'add')
  clickTab(app, 'add')

  press(app, 'i', cmd)
  sockets[1].onclose({ code: 1000, reason: '' })
  assert.equal(app.elements.get('term-rename'), undefined)
  assert.deepEqual(tabNames(app), ['shell 1*'])

  press(app, 'i', cmd)
  app.elements.get('term-rename').value = 'build'
  navigate(app, '#/')
  navigate(app, worktreeHashOf(worktreePath))
  assert.equal(app.elements.get('term-rename'), undefined)
  assert.deepEqual(tabNames(app), ['shell 1*'])
})

// Returns `worktreeApp()` on the repos page with `worktreePath` selected.
async function selectedWorktreeApp() {
  const stubs = await worktreeApp()
  const { app } = stubs
  navigate(app, '#/')

  const index = Array.from(app.run('shownWorktrees()')).findIndex(({ worktree }) => worktree.path === worktreePath)
  for (let i = 0; i <= index; i += 1) press(app, 'ArrowDown')
  assert.equal(app.run('selectedWorktree'), worktreePath)
  return stubs
}

test('o, i and e post to the worktree open route and stay on the repos page', async () => {
  const { app, posts, sockets } = await selectedWorktreeApp()

  for (const key of ['o', 'i', 'e']) assert.ok(press(app, key), `${key} is taken`)
  await settle()
  assert.deepEqual(posts, ['github', 'intellij', 'zed'].map((tool) => `api/open/${tool}?path=${encodedWorktree}`))
  assert.equal(app.context.location.hash, '#/')
  assert.equal(sockets.length, 0)
})

test('t opens the worktree page with one shell', async () => {
  const { app, sockets, terminals } = await selectedWorktreeApp()

  assert.ok(press(app, 't'))
  assert.equal(app.context.location.hash, worktreeHashOf(worktreePath))
  assert.equal(sockets.length, 1)
  assert.equal(sockets[0].url, `ws://127.0.0.1:8731/api/terminal?path=${encodedWorktree}&program=shell&cols=100&rows=30`)
  assert.deepEqual(tabNames(app), ['shell 1*'])
  assert.equal(terminals[0].focused, true)

  assert.ok(press(app, 'Escape'))
  app.window.dispatch('hashchange')
  assert.ok(press(app, 't'))
  assert.equal(app.context.location.hash, worktreeHashOf(worktreePath))
  assert.equal(sockets.length, 1)
  assert.deepEqual(tabNames(app), ['shell 1*'])
})

test('t keeps the active shell', async () => {
  const { app } = await selectedWorktreeApp()
  press(app, 't')
  clickTab(app, 'add')
  const [, second] = tabsOf(app)
  clickTab(app, 'select', second.id)

  navigate(app, '#/')
  assert.ok(press(app, 't'))
  assert.deepEqual(tabNames(app), ['shell 1', 'shell 2*'])
})

test('t with only lazygit open opens a shell', async () => {
  const { app } = await selectedWorktreeApp()

  assert.ok(press(app, 'g'))
  assert.ok(press(app, 't'))
  assert.deepEqual(tabNames(app), ['lazygit', 'shell 1*'])
  assert.deepEqual(tabsOf(app).map((tab) => tab.program), ['lazygit', 'shell'])
})

test('g opens lazygit once', async () => {
  const { app, sockets } = await selectedWorktreeApp()

  assert.ok(press(app, 'g'))
  assert.ok(press(app, 'g'))
  assert.equal(sockets.length, 1)
  assert.equal(sockets[0].url, `ws://127.0.0.1:8731/api/terminal?path=${encodedWorktree}&program=lazygit&cols=100&rows=30`)
  assert.deepEqual(tabNames(app), ['lazygit*'])
})

test('t opens a new shell beside a failed one', async () => {
  const { app, sockets } = await selectedWorktreeApp()
  assert.ok(press(app, 't'))
  sockets[0].onclose({ code: 1006, reason: '' })

  navigate(app, '#/')
  assert.ok(press(app, 't'))
  assert.equal(sockets.length, 2)
  assert.deepEqual(tabNames(app), ['shell 1!', 'shell 2*'])
})

test('g opens a new lazygit beside a failed one', async () => {
  const { app, sockets } = await selectedWorktreeApp()
  assert.ok(press(app, 'g'))
  sockets[0].onclose({ code: 1006, reason: '' })

  navigate(app, '#/')
  assert.ok(press(app, 'g'))
  assert.equal(sockets.length, 2)
  assert.deepEqual(tabNames(app), ['lazygit!', 'lazygit*'])
})

test('the worktree keys work on a worktree page', async () => {
  const { app, posts } = await worktreeApp()

  assert.ok(press(app, 'o'))
  await settle()
  assert.deepEqual(posts, [`api/open/github?path=${encodedWorktree}`])

  assert.ok(press(app, 't'))
  assert.equal(app.context.location.hash, worktreeHashOf(worktreePath))
  assert.deepEqual(tabNames(app), ['shell 1*'])
})

test('the action keys need a selection', async () => {
  const { app, posts, sockets } = await worktreeApp()
  navigate(app, '#/')

  assert.equal(app.run('selectedWorktree'), null)
  for (const key of ['t', 'o']) assert.equal(press(app, key), false, `${key} needs a selection`)
  await settle()
  assert.deepEqual(posts, [])
  assert.equal(sockets.length, 0)
  assert.equal(app.context.location.hash, '#/')
})

test('the action keys do nothing on an unknown worktree page', async () => {
  const { app, posts, sockets } = await worktreeApp('/s/o/r')

  for (const key of ['t', 'g', 'o']) assert.equal(press(app, key), false, `${key} needs a configured worktree`)
  await settle()
  assert.deepEqual(posts, [])
  assert.equal(sockets.length, 0)
  assert.equal(app.context.location.hash, worktreeHashOf('/s/o/r'))

  assert.ok(press(app, 'Escape'))
  assert.equal(app.context.location.hash, '#/')
})

test("the worktree card's lazygit button still opens a new tab", async () => {
  const { app, sockets } = await worktreeApp()

  clickCard(app, 'lazygit')
  clickCard(app, 'lazygit')
  assert.equal(sockets.length, 2)
  assert.deepEqual(tabNames(app), ['lazygit', 'lazygit*'])
})

const worktreeSession = { ...session, org: 'o', repo: 'r', id: 's1', dir: 'api/file/o/r/main/s1/', worktree: worktreePath }

// Goes from `from` to the page of `worktreeSession` and returns the hash a back key then sets.
async function backFromSession(app, from) {
  navigate(app, from)
  navigate(app, '#/o/r/main/s1')
  await settle()

  assert.ok(press(app, 'Escape'))
  return app.context.location.hash
}

test('a back key on a session opened from a worktree page returns to it', async () => {
  const { app } = await worktreeApp(worktreePath, [worktreeSession])
  navigate(app, '#/o/r/main/s1')
  await settle()

  assert.ok(press(app, 'Escape'))
  assert.equal(app.context.location.hash, worktreeHashOf(worktreePath))
  assert.equal(app.run('selectedDir'), worktreeSession.dir)
})

test('a file of that session keeps the origin', async () => {
  const { app } = await worktreeApp(worktreePath, [worktreeSession])
  navigate(app, '#/o/r/main/s1')
  await settle()
  navigate(app, '#/o/r/main/s1/spec.md')
  await settle()

  assert.ok(press(app, 'Escape'))
  assert.equal(app.context.location.hash, worktreeHashOf(worktreePath))
})

test('a session opened elsewhere returns to the sessions tab', async () => {
  const { app } = await worktreeApp(worktreePath, [worktreeSession])

  assert.equal(await backFromSession(app, '#/sessions'), '#/sessions')
  assert.equal(await backFromSession(app, '#/'), '#/sessions')

  navigate(app, worktreeHashOf(worktreePath))
  navigate(app, '#/o/r/main/s1')
  await settle()
  assert.equal(await backFromSession(app, '#/sessions'), '#/sessions')
})
