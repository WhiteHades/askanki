import type { CardContext } from '@/types'

function normalize(value: string, limit = 16_384) {
  return value.replace(/\s+/g, ' ').trim().slice(0, limit)
}

function fieldText(root: Element | null, selectors: string[]) {
  for (const selector of selectors) {
    const value = normalize(root?.querySelector(selector)?.textContent ?? '')
    if (value) return value
  }
  return ''
}

function collect(root: Element | null, selector: string, limit = 32) {
  return [...new Set([...(root?.querySelectorAll(selector) ?? [])]
    .map((element) => normalize(element.textContent ?? ''))
    .filter(Boolean))]
    .slice(0, limit)
}

function readCard(noteId: string | null): CardContext {
  const qa = document.querySelector('#qa')
  const text = normalize(qa?.textContent ?? '', 65_536)
  const children = [...(qa?.children ?? [])]
  const front = fieldText(qa, ['.front', '[data-field="front"]', '.field-front']) || normalize(children[0]?.textContent ?? '')
  const back = fieldText(qa, ['.back', '[data-field="back"]', '.field-back']) || normalize(children.at(-1)?.textContent ?? '')
  const math = collect(qa, '.MathJax, math, .math, [data-type="math"]')
  const code = collect(qa, 'pre, code')
  const tables = collect(qa, 'table')
  const imageLabels = [...(qa?.querySelectorAll('img') ?? [])]
    .map((image) => normalize(image.getAttribute('alt') ?? '', 200))
    .filter(Boolean)
    .slice(0, 32)
  const html = qa?.innerHTML ?? ''
  const context = {
    noteId,
    text,
    front,
    back,
    math,
    code,
    tables,
    imageLabels,
    imageCount: qa?.querySelectorAll('img').length ?? 0,
    hasImages: (qa?.querySelectorAll('img').length ?? 0) > 0,
    hasMath: math.length > 0 || /MathJax|\\\(|\\\[|\\frac|\\sum|\\int/.test(text),
    hasCode: /```|<pre|<code>/.test(html),
  }
  return { ...context, signature: JSON.stringify(context) }
}

export function getCardContext(noteId: string | null) {
  return readCard(noteId)
}

export function observeCardContext(noteId: string | null, onChange: (context: CardContext) => void) {
  const target = document.querySelector('#qa') ?? document.body
  let previous = getCardContext(noteId)
  const observer = new MutationObserver(() => {
    const next = getCardContext(noteId)
    if (next.signature === previous.signature) return
    previous = next
    onChange(next)
  })
  observer.observe(target, { childList: true, subtree: true, characterData: true, attributes: true, attributeFilter: ['src', 'alt'] })
  return () => observer.disconnect()
}
