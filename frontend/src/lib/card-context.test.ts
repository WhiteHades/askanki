import { afterEach, describe, expect, it } from 'vitest'

import { getCardContext } from './card-context'

afterEach(() => {
  document.body.innerHTML = ''
})

describe('getCardContext', () => {
  it('normalizes structured card sections and markers', () => {
    document.body.innerHTML = '<div id="qa"><div class="front">Bonjour \\(x + 1\\)</div><div class="back"><img src="card.png" alt="diagram"><table><tr><td>cell</td></tr></table><pre>const answer = 42</pre></div></div>'

    const context = getCardContext('123')
    expect(context).toEqual(expect.objectContaining({
      noteId: '123',
      text: 'Bonjour \\(x + 1\\)cellconst answer = 42',
      front: 'Bonjour \\(x + 1\\)',
      back: 'cellconst answer = 42',
      math: [],
      code: ['const answer = 42'],
      tables: ['cell'],
      imageLabels: ['diagram'],
      imageCount: 1,
      hasImages: true,
      hasMath: true,
      hasCode: true,
    }))
    expect(context.signature).toBeTruthy()
  })
})
