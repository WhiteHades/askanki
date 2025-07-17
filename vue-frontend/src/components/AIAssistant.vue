<script setup>
import { ref, computed, onMounted, nextTick } from 'vue'
import { marked } from 'marked'
import { GoogleGenAI } from '@google/genai'
import { 
  Cog6ToothIcon,
  TrashIcon,
  XMarkIcon,
  SparklesIcon,
  ArrowUpIcon
} from '@heroicons/vue/24/outline'

const cardContent = ref('')
const messages = ref([])
const isLoading = ref(false)
const showSettings = ref(false)
const apiKey = ref('')
const modelName = ref('gemini-2.5-flash-lite-preview-06-17')
const needsApiKey = ref(false)
const currentCardContent = ref('')
const hasAutoSent = ref(false)
const messagesContainer = ref(null)
const isAutoAnalysis = ref(false)
const currentCardMessages = ref([])
const showExpandedControls = ref(false)

const hasMessages = computed(() => messages.value.length > 0)
const modelDisplayName = computed(() => {
  const name = modelName.value.toLowerCase()
  return name.includes('gemini') ? 'g' : name.includes('gpt') ? 'gpt' : name.includes('claude') ? 'c' : 'ai'
})

marked.setOptions({ breaks: true, gfm: true })

const languageLearningPrompt = `You are an expert language tutor. When the content appears to be language learning related, ALWAYS follow this exact structure:

**Entry:** The word/sentence/phrase in the target language. Use bold.
**Translation (Literal & Natural):** - Literal English translation (word-for-word if useful) - Natural English equivalent (how it's actually said)
**Part of Speech & Form:** - What part of speech is it? (noun, verb, phrase, etc.) - Any important conjugation/form notes
**Pronunciation:** - IPA or basic phonetic help - Stress/syllables if tricky
**Grammar Explanation:** - What grammar rules are involved? - Explain conjugations, particles, tense, structure
**Usage & Context:** - When and how is this used in real conversation? - Formal/casual? Regional?
**Example Sentences:** - 2–3 short example sentences using the word/phrase naturally - Include translations
**Memory Tip:** - Fun or visual trick to help remember it`

const getCurrentCardContent = () => {
  try {
    const qaElement = document.querySelector('#qa')
    if (!qaElement) return { text: '', images: [], hasImages: false }
    
    const text = qaElement.innerText.trim()
    const images = Array.from(qaElement.querySelectorAll('img')).map(img => ({
      src: img.src,
      alt: img.alt || '',
      naturalWidth: img.naturalWidth,
      naturalHeight: img.naturalHeight,
      complete: img.complete
    }))
    
    return { text, images, hasImages: images.length > 0 }
  } catch (error) {
    console.error('could not get card content:', error)
    return { text: '', images: [], hasImages: false }
  }
}

const loadConfig = async () => {
  try {
    let savedApiKey, savedModelName
    try {
      savedApiKey = localStorage.getItem('ai-assistant-api-key')
      savedModelName = localStorage.getItem('ai-assistant-model-name')
    } catch (e) {
      savedApiKey = window.aiAssistantApiKey
      savedModelName = window.aiAssistantModelName
    }
    
    needsApiKey.value = !savedApiKey
    if (savedApiKey) apiKey.value = savedApiKey
    if (savedModelName) modelName.value = savedModelName
  } catch (error) {
    console.error('error loading config:', error)
    needsApiKey.value = true
  }
}

const saveSettings = () => {
  if (!apiKey.value.trim()) return
  
  try {
    try {
      localStorage.setItem('ai-assistant-api-key', apiKey.value.trim())
      localStorage.setItem('ai-assistant-model-name', modelName.value)
    } catch (e) {
      window.aiAssistantApiKey = apiKey.value.trim()
      window.aiAssistantModelName = modelName.value
    }
    
    needsApiKey.value = showSettings.value = hasAutoSent.value = false
    
    if (cardContent.value?.trim?.() || currentCardContent.value?.text?.trim?.()) {
      setTimeout(autoSendCardAnalysis, 300)
    }
  } catch (error) {
    console.error('could not save settings:', error)
  }
}

const scrollToBottom = async () => {
  await nextTick()
  messagesContainer.value?.scrollTo(0, messagesContainer.value.scrollHeight)
}

const buildContextualPrompt = (currentPrompt, isAutoAnalysis = false) => {
  if (isAutoAnalysis) return `${languageLearningPrompt}\n\ncurrent request: ${currentPrompt}`
  
  const cardMessages = currentCardMessages.value.slice(-8)
  if (cardMessages.length > 0) {
    const context = cardMessages.map(m => `${m.type === 'user' ? 'user' : 'assistant'}: ${m.content}`).join('\n')
    return `you are a helpful language learning assistant. continue the conversation naturally based on the context.\n\nconversation context:\n${context}\n\nuser: ${currentPrompt}`
  }
  
  return `you are a helpful language learning assistant. answer naturally and conversationally.\n\nuser: ${currentPrompt}`
}

const imageToBase64 = async (src) => {
  return new Promise((resolve, reject) => {
    if (src.startsWith('data:')) {
      try {
        const base64 = src.split(',')[1]
        if (!base64) throw new Error('invalid data url format')
        resolve(base64)
      } catch (error) {
        reject(error)
      }
      return
    }
    
    const img = new Image()
    img.crossOrigin = 'anonymous'
    
    img.onload = () => {
      try {
        const canvas = document.createElement('canvas')
        const ctx = canvas.getContext('2d')
        canvas.width = img.naturalWidth
        canvas.height = img.naturalHeight
        ctx.drawImage(img, 0, 0)
        resolve(canvas.toDataURL('image/jpeg', 0.8).split(',')[1])
      } catch (error) {
        fetchFallback()
      }
    }
    
    img.onerror = fetchFallback
    img.src = src
    
    async function fetchFallback() {
      try {
        const response = await fetch(src, { mode: 'cors', credentials: 'omit' })
        if (!response.ok) throw new Error(`http ${response.status}`)
        
        const blob = await response.blob()
        const reader = new FileReader()
        reader.onload = () => resolve(reader.result.split(',')[1])
        reader.onerror = reject
        reader.readAsDataURL(blob)
      } catch (error) {
        reject(error)
      }
    }
  })
}

const callGeminiAPI = async (prompt, cardData = null) => {
  const ai = new GoogleGenAI({ apiKey: apiKey.value })
  
  try {
    const contentParts = [{ text: prompt }]
    
    if (cardData?.hasImages) {
      for (const image of cardData.images) {
        try {
          const base64Data = await imageToBase64(image.src)
          const extension = image.src.toLowerCase().split('.').pop()?.split('?')[0]
          const mimeTypes = { png: 'image/png', jpg: 'image/jpeg', jpeg: 'image/jpeg', webp: 'image/webp', heic: 'image/heic', heif: 'image/heif' }
          const mimeType = image.src.startsWith('data:') ? image.src.match(/data:([^;]+)/)?.[1] || 'image/jpeg' : mimeTypes[extension] || 'image/jpeg'
          
          contentParts.push({ inlineData: { mimeType, data: base64Data } })
        } catch (error) {
          console.error('failed to process image:', error)
        }
      }
    }
    
    const response = await ai.models.generateContent({
      model: modelName.value,
      contents: [{ parts: contentParts }],
      generationConfig: { temperature: 0.7, topK: 40, topP: 0.95, maxOutputTokens: 2048 }
    })
    
    return response.text
  } catch (error) {
    console.error('api call failed:', error)
    throw error
  }
}

const createMessage = (type, content) => ({ id: Date.now() + Math.random(), type, content, timestamp: new Date() })

const sendMessage = async () => {
  if (!cardContent.value.trim()) return
  if (needsApiKey.value) { showSettings.value = true; return }
  
  const currentPrompt = cardContent.value
  const userMessage = createMessage('user', currentPrompt)
  messages.value.push(userMessage)
  currentCardMessages.value.push(userMessage)
  cardContent.value = ''
  isLoading.value = true
  scrollToBottom()
  
  try {
    const cardData = isAutoAnalysis.value && currentCardContent.value?.hasImages ? currentCardContent.value : null
    const aiResponse = await callGeminiAPI(buildContextualPrompt(currentPrompt, isAutoAnalysis.value), cardData)
    const aiMessage = createMessage('ai', aiResponse)
    messages.value.push(aiMessage)
    currentCardMessages.value.push(aiMessage)
    isAutoAnalysis.value = false
  } catch (error) {
    const errorMessage = createMessage('error', 'error: could not get ai response. please check your api key and try again.')
    messages.value.push(errorMessage)
    currentCardMessages.value.push(errorMessage)
  } finally {
    isLoading.value = false
    scrollToBottom()
  }
}

const clearChat = () => {
  messages.value = []
  currentCardMessages.value = []
  cardContent.value = ''
  isAutoAnalysis.value = false
}

const triggerAnalysis = (isManual = false) => {
  if (needsApiKey.value || isLoading.value) return
  
  if (isManual) currentCardContent.value = getCurrentCardContent()
  
  const hasText = cardContent.value.trim() || currentCardContent.value?.text?.trim()
  const hasImages = currentCardContent.value?.hasImages
  
  if (!hasText && !hasImages) return
  
  currentCardMessages.value = []
  isAutoAnalysis.value = true
  
  const imageCount = currentCardContent.value?.images?.length || 0
  const imageText = imageCount > 0 ? ` (${imageCount} image${imageCount > 1 ? 's' : ''})` : ''
  
  let prompt = 'analyze this flashcard content'
  if (hasImages && hasText) prompt += ` including any images${imageText}: ${hasText}`
  else if (hasImages) prompt += `${imageText} (image-only card)`
  else prompt += `: ${hasText}`
  
  cardContent.value = prompt
  sendMessage()
}

const autoSendCardAnalysis = () => triggerAnalysis(false)
const manualAnalysis = () => triggerAnalysis(true)

const toggleExpandedControls = () => {
  showExpandedControls.value = !showExpandedControls.value
}

const closeExpandedControls = () => {
  showExpandedControls.value = false
}

const autoResize = (event) => {
  const textarea = event.target
  textarea.style.height = 'auto'
  textarea.style.height = Math.min(textarea.scrollHeight, 100) + 'px'
}

const renderMarkdown = (content) => marked(content)

onMounted(() => {
  const content = getCurrentCardContent()
  if (content.text || content.hasImages) {
    cardContent.value = content.text
    currentCardContent.value = content
  }
  
  loadConfig()
  window.addEventListener('ai-panel-opened', () => {
    const hasText = cardContent.value?.trim?.() || currentCardContent.value?.text?.trim?.()
    const hasImages = currentCardContent.value?.hasImages
    
    if (!hasAutoSent.value && !needsApiKey.value && (hasText || hasImages)) {
      currentCardMessages.value = []
      autoSendCardAnalysis()
      hasAutoSent.value = true
    } else if (needsApiKey.value) {
      showSettings.value = true
    }
  })
  
  const observer = new MutationObserver(() => {
    const newCardContent = getCurrentCardContent()
    const oldCardContent = currentCardContent.value
    
    const contentChanged = newCardContent?.text !== oldCardContent?.text || newCardContent?.images?.length !== oldCardContent?.images?.length
    
    if (newCardContent && contentChanged) {
      currentCardContent.value = newCardContent
      cardContent.value = newCardContent.text || ''
      hasAutoSent.value = false
      currentCardMessages.value = []
      
      if (window.aiPanelVisible && !needsApiKey.value && (newCardContent.text || newCardContent.hasImages)) {
        autoSendCardAnalysis()
      }
    }
  })
  
  const qaElement = document.querySelector('#qa') || document.body
  observer.observe(qaElement, { childList: true, subtree: true })
})
</script>

<template>
  <div class="grok-chat">
    <div v-if="showSettings" class="modal-overlay" @click.self="!needsApiKey && (showSettings = false)">
      <div class="modal">
        <div class="modal-header">
          <h3>{{ needsApiKey ? 'Setup Required' : 'Settings' }}</h3>
        </div>
        
        <div class="modal-body">
          <div class="field">
            <label>api key</label>
            <input 
              v-model="apiKey" 
              type="password" 
              placeholder="Enter API key..."
              class="input"
            >
          </div>
          
          <div class="field">
            <label>model</label>
            <select v-model="modelName" class="input">
              <option value="gemini-2.5-flash-lite-preview-06-17">gemini 2.5 flash</option>
            </select>
          </div>
          
          <button @click="saveSettings" class="btn-primary" :disabled="!apiKey.trim()">
            {{ needsApiKey ? 'start' : 'save' }}
          </button>
        </div>
      </div>
    </div>

    <div v-if="showExpandedControls" class="expanded-overlay" @click="closeExpandedControls"></div>
    
    <div class="expandable-controls" :class="{ 'expanded': showExpandedControls }">
      <transition name="controls-fade">
        <div v-if="showExpandedControls" class="expanded-menu">
          <button @click="manualAnalysis(); showExpandedControls = false" class="action-btn" title="analyze">
            <SparklesIcon class="action-icon" />
          </button>
          <button @click="clearChat(); showExpandedControls = false" class="action-btn" :disabled="!hasMessages" title="clear">
            <TrashIcon class="action-icon" />
          </button>
          <button @click="showSettings = true; showExpandedControls = false" class="action-btn" title="settings">
            <Cog6ToothIcon class="action-icon" />
          </button>
          <button @click="$emit('close')" class="action-btn close-btn" title="close">
            <XMarkIcon class="action-icon" />
          </button>
        </div>
      </transition>
    </div>

    <div class="messages" ref="messagesContainer">
      <div v-if="!hasMessages && !isLoading" class="empty">
        <p class="empty-text">ready</p>
      </div>

      <div 
        v-for="message in messages" 
        :key="message.id"
        :class="['msg', `msg-${message.type}`]"
      >
        <div 
          v-if="message.type === 'ai'"
          class="msg-content markdown-content" 
          v-html="renderMarkdown(message.content)"
        ></div>
        <div 
          v-else
          class="msg-content"
        >{{ message.content }}</div>
        <div class="msg-time">{{ formatTime(message.timestamp) }}</div>
      </div>
      
      <div v-if="isLoading" class="msg msg-ai">
        <div class="typing">
          <span></span><span></span><span></span>
        </div>
      </div>
    </div>

    <div class="input-area">
      <div class="input-container">
        <button @click="toggleExpandedControls" class="settings-btn" title="settings">
          <Cog6ToothIcon class="settings-icon" />
        </button>
        
        <textarea
          v-model="cardContent"
          placeholder="message"
          class="textarea"
          rows="1"
          @keydown.enter.exact.prevent="sendMessage"
          @input="autoResize"
        ></textarea>
        
        <button @click="sendMessage" class="send-btn" :disabled="!cardContent.trim() || isLoading">
          <ArrowUpIcon class="send-icon" />
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.grok-chat {
  height: 100%;
  display: flex;
  flex-direction: column;
  font-family: BlinkMacSystemFont, 'Segoe UI', system-ui, sans-serif;
  background: transparent;
  font-size: 12px;
  overflow: hidden;
  position: relative;
}

.modal-overlay {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.75);
  backdrop-filter: blur(4px);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 1000;
  animation: fade-in 0.15s ease;
}

.modal {
  background: rgba(20, 20, 22, 0.95);
  border: 1px solid rgba(255, 255, 255, 0.1);
  border-radius: 8px;
  padding: 20px;
  min-width: 300px;
  animation: slide-up 0.2s ease;
}

.modal-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 16px;
}

.modal-header h3 {
  margin: 0;
  color: white;
  font-size: 13px;
  font-weight: 600;
}

.modal-body {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.field {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.field label {
  color: rgba(255, 255, 255, 0.8);
  font-size: 10px;
  font-weight: 500;
}

.input {
  background: rgba(255, 255, 255, 0.05);
  border: 1px solid rgba(255, 255, 255, 0.1);
  border-radius: 4px;
  padding: 6px 10px;
  color: white;
  font-size: 11px;
  transition: border-color 0.15s ease;
}

.input:focus {
  outline: none;
  border-color: rgba(255, 255, 255, 0.3);
}

.btn-primary {
  background: rgba(255, 255, 255, 0.9);
  color: black;
  border: none;
  border-radius: 4px;
  padding: 6px 12px;
  font-size: 11px;
  font-weight: 600;
  cursor: pointer;
  transition: all 0.15s ease;
}

.btn-primary:hover:not(:disabled) {
  background: white;
}

.btn-primary:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.expanded-overlay {
  position: absolute;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  z-index: 15;
  background: transparent;
}

.expandable-controls {
  position: absolute;
  top: 8px;
  right: 8px;
  z-index: 20;
}

.expanded-menu {
  position: absolute;
  top: 32px;
  right: 0;
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 12px;
  background: rgba(0, 0, 0, 0.85);
  backdrop-filter: blur(20px) saturate(180%);
  border: 1px solid rgba(255, 255, 255, 0.1);
  border-radius: 12px;
  box-shadow: 
    0 8px 32px rgba(0, 0, 0, 0.4),
    0 4px 16px rgba(0, 0, 0, 0.2);
}

.action-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  background: rgba(255, 255, 255, 0.05);
  border: 1px solid rgba(255, 255, 255, 0.1);
  color: rgba(255, 255, 255, 0.8);
  cursor: pointer;
  border-radius: 8px;
  transition: all 0.2s cubic-bezier(0.4, 0.0, 0.2, 1);
  padding: 2px;
}

.action-btn:hover:not(:disabled) {
  background: rgba(255, 255, 255, 0.15);
  color: rgba(255, 255, 255, 0.95);
  border-color: rgba(255, 255, 255, 0.2);
  transform: translateY(-1px);
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.3);
}

.action-btn:disabled {
  opacity: 0.3;
  cursor: not-allowed;
}

.action-btn.close-btn {
  background: rgba(220, 38, 38, 0.1);
  border-color: rgba(220, 38, 38, 0.2);
  color: rgba(255, 100, 100, 0.9);
}

.action-btn.close-btn:hover {
  background: rgba(220, 38, 38, 0.2);
  border-color: rgba(220, 38, 38, 0.4);
  color: rgba(255, 120, 120, 0.95);
}

.action-icon {
  width: 16px;
  height: 16px;
}

.controls-fade-enter-active,
.controls-fade-leave-active {
  transition: all 0.3s cubic-bezier(0.4, 0.0, 0.2, 1);
}

.controls-fade-enter-from {
  opacity: 0;
  transform: translateY(-8px) scale(0.95);
}

.controls-fade-leave-to {
  opacity: 0;
  transform: translateY(-8px) scale(0.95);
}

.messages {
  flex: 1;
  overflow-y: auto;
  padding: 28px 12px 4px 12px;
  scroll-behavior: smooth;
}

.messages::-webkit-scrollbar {
  width: 4px;
}

.messages::-webkit-scrollbar-track {
  background: transparent;
}

.messages::-webkit-scrollbar-thumb {
  background: rgba(255, 255, 255, 0.1);
  border-radius: 2px;
}

.empty {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  height: 100%;
  text-align: center;
}

.empty-text {
  color: rgba(255, 255, 255, 0.5);
  font-size: 11px;
  margin: 0;
}

.msg {
  margin-bottom: 8px;
  max-width: 85%;
  opacity: 0;
  animation: fade-slide-in 0.3s ease forwards;
}

.msg-user {
  margin-left: auto;
}

.msg-ai, .msg-error {
  margin-right: auto;
}

.msg-content {
  padding: 8px 12px;
  border-radius: 12px;
  font-size: 11px;
  line-height: 1.3;
  word-wrap: break-word;
}

.msg-user .msg-content {
  background: rgba(255, 255, 255, 0.9);
  color: black;
  border-bottom-right-radius: 4px;
}

.msg-ai .msg-content {
  background: rgba(255, 255, 255, 0.05);
  color: rgba(255, 255, 255, 0.95);
  border: 1px solid rgba(255, 255, 255, 0.1);
  border-bottom-left-radius: 4px;
}

.msg-error .msg-content {
  background: rgba(239, 68, 68, 0.1);
  color: rgba(248, 113, 113, 0.9);
  border: 1px solid rgba(239, 68, 68, 0.2);
}

.markdown-content {
  font-family: inherit;
  text-align: left !important;
}

.markdown-content h1,
.markdown-content h2,
.markdown-content h3,
.markdown-content h4 {
  margin: 0.4em 0 0.2em 0;
  color: rgba(255, 255, 255, 0.95);
  font-weight: 600;
  text-align: left;
}

.markdown-content h1 { font-size: 14px; }
.markdown-content h2 { font-size: 13px; }
.markdown-content h3 { font-size: 12px; }
.markdown-content h4 { font-size: 11px; }

.markdown-content p {
  margin: 0.2em 0;
  line-height: 1.3;
  text-align: left;
}

.markdown-content ul,
.markdown-content ol {
  margin: 0.2em 0;
  padding-left: 1em;
  text-align: left;
}

.markdown-content li {
  margin: 0.05em 0;
  text-align: left;
}

.markdown-content strong {
  color: rgba(255, 255, 255, 1);
  font-weight: 600;
  text-align: left;
}

.markdown-content em {
  font-style: italic;
  color: rgba(255, 255, 255, 0.9);
}

.markdown-content code {
  background: rgba(255, 255, 255, 0.1);
  padding: 0.1em 0.2em;
  border-radius: 2px;
  font-family: 'SF Mono', Monaco, monospace;
  font-size: 9px;
  text-align: left;
}

.markdown-content pre {
  background: rgba(255, 255, 255, 0.05);
  padding: 0.3em;
  border-radius: 3px;
  overflow-x: auto;
  margin: 0.2em 0;
}

.markdown-content blockquote {
  border-left: 2px solid rgba(255, 255, 255, 0.3);
  padding-left: 0.3em;
  margin: 0.2em 0;
  color: rgba(255, 255, 255, 0.8);
}

.msg-time {
  font-size: 9px;
  color: rgba(255, 255, 255, 0.4);
  margin-top: 2px;
  text-align: right;
}

.msg-ai .msg-time, .msg-error .msg-time {
  text-align: left;
}

.typing {
  display: flex;
  align-items: center;
  gap: 2px;
  padding: 12px 16px;
  background: rgba(255, 255, 255, 0.05);
  border: 1px solid rgba(255, 255, 255, 0.1);
  border-radius: 12px;
  border-bottom-left-radius: 4px;
}

.typing span {
  width: 4px;
  height: 4px;
  background: rgba(255, 255, 255, 0.6);
  border-radius: 50%;
  animation: typing-pulse 1.4s infinite ease-in-out;
}

.typing span:nth-child(2) {
  animation-delay: 0.2s;
}

.typing span:nth-child(3) {
  animation-delay: 0.4s;
}

.input-area {
  padding: 8px 12px;
}

.input-container {
  background: rgba(255, 255, 255, 0.08);
  border: 1px solid rgba(255, 255, 255, 0.12);
  border-radius: 20px;
  padding: 4px;
  display: flex;
  align-items: flex-end;
  gap: 4px;
  transition: all 0.2s ease;
  min-height: 36px;
  position: relative;
}

.input-container:focus-within {
  border-color: rgba(255, 255, 255, 0.25);
  background: rgba(255, 255, 255, 0.1);
}

.settings-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 22px;
  height: 22px;
  background: rgba(255, 255, 255, 0.1);
  border: none;
  border-radius: 11px;
  color: rgba(255, 255, 255, 0.7);
  cursor: pointer;
  transition: all 0.2s cubic-bezier(0.4, 0.0, 0.2, 1);
  flex-shrink: 0;
  margin: 2px;
  padding: 1px;
}

.settings-btn:hover {
  background: rgba(255, 255, 255, 0.2);
  color: rgba(255, 255, 255, 0.9);
  transform: scale(1.05);
}

.settings-icon {
  width: 14px;
  height: 14px;
}

.textarea {
  flex: 1;
  background: none;
  border: none;
  color: white;
  font-size: 11px;
  line-height: 1.3;
  resize: none;
  outline: none;
  max-height: 80px;
  font-family: inherit;
  padding: 6px 4px;
  margin: 2px 0;
}

.textarea::placeholder {
  color: rgba(255, 255, 255, 0.5);
}

.send-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 22px;
  height: 22px;
  background: rgba(255, 255, 255, 0.6);
  border: none;
  border-radius: 11px;
  color: rgba(0, 0, 0, 0.8);
  cursor: pointer;
  transition: all 0.2s cubic-bezier(0.4, 0.0, 0.2, 1);
  flex-shrink: 0;
  margin: 2px;
  padding: 1px;
}

.send-btn:hover:not(:disabled) {
  background: rgba(255, 255, 255, 0.8);
  color: black;
  transform: translateY(-1px);
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.2);
}

.send-btn:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}

.send-icon {
  width: 14px;
  height: 14px;
}

@keyframes fade-in {
  from { opacity: 0; }
  to { opacity: 1; }
}

@keyframes slide-up {
  from { 
    opacity: 0; 
    transform: translateY(12px) scale(0.98); 
  }
  to { 
    opacity: 1; 
    transform: translateY(0) scale(1); 
  }
}

@keyframes fade-slide-in {
  from { 
    opacity: 0; 
    transform: translateY(8px); 
  }
  to { 
    opacity: 1; 
    transform: translateY(0); 
  }
}

@keyframes typing-pulse {
  0%, 60%, 100% { 
    transform: translateY(0); 
    opacity: 0.4; 
  }
  30% { 
    transform: translateY(-2px); 
    opacity: 1; 
  }
}
</style>

<script>
export default {
  methods: {
    formatTime(timestamp) {
      return new Date(timestamp).toLocaleTimeString([], { 
        hour: '2-digit', 
        minute: '2-digit' 
      })
    }
  }
}
</script> 