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

function dashboard({ script = source, pageWidth = 1200, pageLeft = 0, splitterWidth = 6, fetcher } = {}) {
  const elements = new Map()
  const calls = []
  const geometry = { pageWidth, pageLeft, splitterWidth }

  const element = (id) => {
    const properties = new Map()
    const classes = new Set()
    const el = Object.assign(new Target(), {
      id,
      style: {
        setProperty: (name, value) => properties.set(name, value),
        getPropertyValue: (name) => properties.get(name) ?? '',
      },
      classList: { add: (name) => classes.add(name), remove: (name) => classes.delete(name) },
      setPointerCapture: () => {},
      querySelector: () => null,
      scrollTop: 0,
      firstElementChild: null,
      innerHTML: '',
      getBoundingClientRect: () => ({
        width: id === 'splitter' ? geometry.splitterWidth : geometry.pageWidth,
        left: geometry.pageLeft,
      }),
    })
    elements.set(id, el)
    return el
  }

  for (const id of ['page', 'font-down', 'font-up', 'font-value']) element(id)

  Object.defineProperty(elements.get('page'), 'innerHTML', {
    set(value) {
      this.firstElementChild = null
      if (!value.includes('class="session-page"')) return

      this.firstElementChild = element('session-page')
      for (const id of ['metro-pane', 'session-card', 'metro', 'splitter', 'viewer-head', 'view']) {
        element(id).parentElement = this.firstElementChild
      }
    },
  })

  const document = Object.assign(new Target(), {
    getElementById: (id) => elements.get(id) ?? null,
    documentElement: { style: {} },
  })
  const window = new Target()
  const context = vm.createContext({
    document,
    window,
    localStorage: { getItem: () => null },
    fetch: async (url) => {
      calls.push(url)
      if (!fetcher) throw new Error(`unexpected fetch: ${url}`)
      return fetcher(url)
    },
    DOMParser: class {
      parseFromString(text) {
        return {
          querySelectorAll: () => [...text.matchAll(/href="([^"]*)"/g)].map((match) => ({ getAttribute: () => match[1] })),
        }
      }
    },
  })
  const run = (code) => vm.runInContext(code, context, { filename: 'dashboard.html' })
  run(script)
  return { run, context, elements, window, calls, geometry }
}

const session = { repo: 'repo', id: 'session', dir: 'repo/sessions/session/', flow: 'build', stage: 'implement', round: 0 }
const response = (text, ok = true) => ({ ok, status: ok ? 200 : 500, text: async () => text })
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
  const app = dashboard({ fetcher: () => response('<a href="session.json">session.json</a>') })
  app.context.session = session
  await app.run('renderSessionPage(session, null)')
  assert.deepEqual(app.calls, [session.dir])
})

test('a listed run log is parsed after listing and tolerates a truncated line', async () => {
  const app = dashboard({
    fetcher: (url) => response(url.endsWith('runs.jsonl')
      ? '{"stop":"spec","round":0,"agent":"writer","tokens":5}\n{"stop":"pl'
      : '<a href="runs.jsonl">runs.jsonl</a>'),
  })
  app.context.session = session
  await app.run('renderSessionPage(session, null)')
  assert.deepEqual(app.calls, [session.dir, `${session.dir}runs.jsonl`])
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
  assert.match(html, /@media \(max-width: 800px\)\s*\{\s*\.session-page \{ grid-template-columns: minmax\(0, 1fr\)/)

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
    [session.dir]: '<a href="round-1/">round-1/</a>',
    [`${session.dir}round-1/`]: '<a href="triage.md">triage.md</a>',
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
