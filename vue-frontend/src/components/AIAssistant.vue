<script setup>
import { ref, computed, onMounted, nextTick } from 'vue'
import { marked } from 'marked'
import { GoogleGenAI } from '@google/genai'
import { 
  Cog6ToothIcon,
  TrashIcon,
  XMarkIcon,
  SparklesIcon,
  ArrowRightIcon
} from '@heroicons/vue/24/outline'

const cardContent = ref('')
const messages = ref([])
const isLoading = ref(false)
const showSettings = ref(false)
const apiKey = ref('')
const modelName = ref('gemini-2.5-flash')
const needsApiKey = ref(false)
const currentCardContent = ref('')
const hasAutoSent = ref(false)
const messagesContainer = ref(null)

const hasMessages = computed(() => messages.value.length > 0)
const modelDisplayName = computed(() => {
  const name = modelName.value.toLowerCase()
  return name.includes('gemini') ? 'g' : name.includes('gpt') ? 'gpt' : name.includes('claude') ? 'c' : 'ai'
})

marked.setOptions({ breaks: true, gfm: true })

const languageLearningPrompt = `You are an expert language tutor. When the content appears to be language learning related, ALWAYS follow this exact structure, if this is language learning content:

**Entry:**
The word/sentence/phrase in the target language. Use bold.

**Translation (Literal & Natural):**
- Literal English translation (word-for-word if useful)
- Natural English equivalent (how it's actually said)

**Part of Speech & Form:**
- What part of speech is it? (noun, verb, phrase, etc.)
- Any important conjugation/form notes

**Pronunciation:**
- IPA or basic phonetic help
- Stress/syllables if tricky

**Grammar Explanation:**
- What grammar rules are involved?
- Explain conjugations, particles, tense, structure

**Usage & Context:**
- When and how is this used in real conversation?
- Formal/casual? Regional?

**Example Sentences:**
- 2–3 short example sentences using the word/phrase naturally
- Include translations

**Memory Tip:**
- Fun or visual trick to help remember it`

onMounted(() => {
  loadCurrentCard()
  loadConfig()
  
  window.addEventListener('ai-panel-opened', handlePanelOpened)
  
  const observer = new MutationObserver(() => {
    const newCardContent = getCurrentCardContent()
    if (newCardContent && newCardContent !== currentCardContent.value) {
      currentCardContent.value = newCardContent
      cardContent.value = newCardContent
      
      if (window.aiPanelVisible && !needsApiKey.value) {
        autoSendCardAnalysis()
      }
    }
  })
  
  const qaElement = document.querySelector('#qa') || document.body
  observer.observe(qaElement, { childList: true, subtree: true })
})

const handlePanelOpened = () => {
  if (!hasAutoSent.value && !needsApiKey.value && cardContent.value.trim()) {
    autoSendCardAnalysis()
    hasAutoSent.value = true
  } else if (needsApiKey.value) {
    showSettings.value = true
  }
}

const getCurrentCardContent = () => {
  try {
    const qaElement = document.querySelector('#qa')
    if (qaElement) {
      return qaElement.innerText.trim()
    }
  } catch (error) {
          console.log('could not get card content:', error)
  }
  return ''
}

const loadCurrentCard = () => {
  const content = getCurrentCardContent()
  if (content) {
    cardContent.value = content
    currentCardContent.value = content
  }
}

const loadConfig = async () => {
  try {
    const savedApiKey = localStorage.getItem('ai-assistant-api-key')
    const savedModelName = localStorage.getItem('ai-assistant-model-name')
    
    if (savedApiKey) {
      apiKey.value = savedApiKey
      needsApiKey.value = false
    } else {
      needsApiKey.value = true
    }
    
    if (savedModelName) {
      modelName.value = savedModelName
    }
  } catch (error) {
    console.log('Could not load config:', error)
    needsApiKey.value = true
  }
}

const scrollToBottom = async () => {
  await nextTick()
  messagesContainer.value?.scrollTo(0, messagesContainer.value.scrollHeight)
}

const buildContextualPrompt = (currentPrompt) => {
  const recentMessages = messages.value.slice(-6)
  const context = recentMessages.length ? 
    `${languageLearningPrompt}\n\nrecent conversation context:\n${recentMessages.map(m => `${m.type === 'user' ? 'user' : 'assistant'}: ${m.content}`).join('\n')}\n\n` : 
    `${languageLearningPrompt}\n\n`
  return `${context}current request: ${currentPrompt}`
}

const callGeminiAPI = async (prompt) => {
  const ai = new GoogleGenAI({ apiKey: apiKey.value })
  const response = await ai.models.generateContent({
    model: modelName.value,
    contents: prompt,
    config: {
      temperature: 0.7,
      topK: 40,
      topP: 0.95,
      maxOutputTokens: 2048,
    }
  })
  return response.text
}

const sendMessage = async () => {
  if (!cardContent.value.trim()) return
  
  if (needsApiKey.value) {
    showSettings.value = true
    return
  }
  
  const currentPrompt = cardContent.value
  const createMessage = (type, content) => ({
    id: Date.now() + Math.random(),
    type,
    content,
    timestamp: new Date()
  })
  
  messages.value.push(createMessage('user', currentPrompt))
  cardContent.value = ''
  isLoading.value = true
  scrollToBottom()
  
  try {
    const aiResponse = await callGeminiAPI(buildContextualPrompt(currentPrompt))
    messages.value.push(createMessage('ai', aiResponse))
  } catch (error) {
    messages.value.push(createMessage('error', 'error: could not get ai response. please check your api key and try again.'))
  } finally {
    isLoading.value = false
    scrollToBottom()
  }
}

const clearChat = () => {
  messages.value = []
  cardContent.value = ''
}

const saveSettings = () => {
  if (!apiKey.value.trim()) return
  
  try {
    localStorage.setItem('ai-assistant-api-key', apiKey.value.trim())
    localStorage.setItem('ai-assistant-model-name', modelName.value)
    needsApiKey.value = showSettings.value = hasAutoSent.value = false
    
    if (cardContent.value.trim()) setTimeout(() => autoSendCardAnalysis(), 300)
  } catch (error) {
    console.error('could not save settings:', error)
  }
}

const autoSendCardAnalysis = async () => {
  if (!cardContent.value.trim() || needsApiKey.value || isLoading.value) return
  cardContent.value = `analyze this flashcard content: ${cardContent.value.trim()}`
  await sendMessage()
}

const autoResize = (event) => {
  const textarea = event.target
  textarea.style.height = 'auto'
  textarea.style.height = Math.min(textarea.scrollHeight, 100) + 'px'
}

const renderMarkdown = (content) => {
  return marked(content)
}
</script>

<template>
  <div class="grok-chat">
    <div v-if="showSettings" class="modal-overlay" @click.self="!needsApiKey && (showSettings = false)">
      <div class="modal">
        <div class="modal-header">
          <h3>{{ needsApiKey ? 'Setup Required' : 'Settings' }}</h3>
          <button v-if="!needsApiKey" @click="showSettings = false" class="modal-close">
            <XMarkIcon class="w-3 h-3" />
          </button>
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
              <option value="gemini-2.5-flash">gemini 2.5 flash</option>
            </select>
          </div>
          
          <button @click="saveSettings" class="btn-primary" :disabled="!apiKey.trim()">
            {{ needsApiKey ? 'start' : 'save' }}
          </button>
        </div>
      </div>
    </div>

    <div class="header">
      <span class="title">{{ modelDisplayName }}</span>
      <div class="header-right">
        <button @click="clearChat" class="header-btn" :disabled="!hasMessages">
          <TrashIcon class="w-2.5 h-2.5" />
        </button>
        <button @click="showSettings = true" class="header-btn">
          <Cog6ToothIcon class="w-2.5 h-2.5" />
        </button>
      </div>
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
        <textarea
          v-model="cardContent"
          placeholder="message"
          class="textarea"
          rows="1"
          @keydown.enter.exact.prevent="sendMessage"
          @input="autoResize"
        ></textarea>
        
        <button @click="sendMessage" class="send-btn" :disabled="!cardContent.trim() || isLoading">
          <ArrowRightIcon class="w-2.5 h-2.5" />
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
  font-size: 11px;
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
  border-radius: 6px;
  padding: 12px;
  min-width: 260px;
  animation: slide-up 0.2s ease;
}

.modal-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 8px;
}

.modal-header h3 {
  margin: 0;
  color: white;
  font-size: 12px;
  font-weight: 600;
}

.modal-close {
  background: none;
  border: none;
  color: rgba(255, 255, 255, 0.6);
  cursor: pointer;
  padding: 2px;
  border-radius: 2px;
  transition: color 0.15s ease;
}

.modal-close:hover {
  color: white;
}

.modal-body {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.field {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.field label {
  color: rgba(255, 255, 255, 0.8);
  font-size: 10px;
  font-weight: 500;
}

.input {
  background: rgba(255, 255, 255, 0.05);
  border: 1px solid rgba(255, 255, 255, 0.1);
  border-radius: 3px;
  padding: 4px 6px;
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
  border-radius: 3px;
  padding: 4px 8px;
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

.header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 4px 8px;
  border-bottom: 1px solid rgba(255, 255, 255, 0.08);
  min-height: 18px;
}

.title {
  color: white;
  font-size: 10px;
  font-weight: 600;
}

.header-right {
  display: flex;
  align-items: center;
  gap: 1px;
}

.header-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 12px;
  height: 12px;
  background: none;
  border: none;
  color: rgba(255, 255, 255, 0.6);
  cursor: pointer;
  border-radius: 2px;
  transition: all 0.15s ease;
}

.header-btn:hover:not(:disabled) {
  background: rgba(255, 255, 255, 0.1);
  color: white;
}

.header-btn:disabled {
  opacity: 0.3;
  cursor: not-allowed;
}

.messages {
  flex: 1;
  overflow-y: auto;
  padding: 8px;
  scroll-behavior: smooth;
}

.messages::-webkit-scrollbar {
  width: 2px;
}

.messages::-webkit-scrollbar-track {
  background: transparent;
}

.messages::-webkit-scrollbar-thumb {
  background: rgba(255, 255, 255, 0.1);
  border-radius: 1px;
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
  font-size: 10px;
  margin: 0;
}

.msg {
  margin-bottom: 6px;
  max-width: 90%;
  opacity: 0;
  animation: fade-slide-in 0.4s ease forwards;
}

.msg-user {
  margin-left: auto;
}

.msg-ai, .msg-error {
  margin-right: auto;
}

.msg-content {
  padding: 4px 8px;
  border-radius: 8px;
  font-size: 11px;
  line-height: 1.3;
  word-wrap: break-word;
}

.msg-user .msg-content {
  background: rgba(255, 255, 255, 0.9);
  color: black;
  border-bottom-right-radius: 2px;
}

.msg-ai .msg-content {
  background: rgba(255, 255, 255, 0.05);
  color: rgba(255, 255, 255, 0.95);
  border: 1px solid rgba(255, 255, 255, 0.1);
  border-bottom-left-radius: 2px;
}

.msg-error .msg-content {
  background: rgba(239, 68, 68, 0.1);
  color: rgba(248, 113, 113, 0.9);
  border: 1px solid rgba(239, 68, 68, 0.2);
}

.markdown-content {
  font-family: inherit;
}

.markdown-content h1,
.markdown-content h2,
.markdown-content h3,
.markdown-content h4 {
  margin: 0.4em 0 0.2em 0;
  color: rgba(255, 255, 255, 0.95);
  font-weight: 600;
}

.markdown-content h1 { font-size: 12px; }
.markdown-content h2 { font-size: 11px; }
.markdown-content h3 { font-size: 11px; }
.markdown-content h4 { font-size: 10px; }

.markdown-content p {
  margin: 0.2em 0;
  line-height: 1.3;
}

.markdown-content ul,
.markdown-content ol {
  margin: 0.2em 0;
  padding-left: 1em;
}

.markdown-content li {
  margin: 0.05em 0;
}

.markdown-content strong {
  color: rgba(255, 255, 255, 1);
  font-weight: 600;
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
  font-size: 10px;
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
  font-size: 8px;
  color: rgba(255, 255, 255, 0.4);
  margin-top: 1px;
  text-align: right;
}

.msg-ai .msg-time, .msg-error .msg-time {
  text-align: left;
}

.typing {
  display: flex;
  align-items: center;
  gap: 1px;
  padding: 4px 8px;
  background: rgba(255, 255, 255, 0.05);
  border: 1px solid rgba(255, 255, 255, 0.1);
  border-radius: 8px;
  border-bottom-left-radius: 2px;
}

.typing span {
  width: 2px;
  height: 2px;
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
  padding: 12px 16px;
}

.input-container {
  background: rgba(255, 255, 255, 0.08);
  border: 1px solid rgba(255, 255, 255, 0.12);
  border-radius: 20px;
  padding: 8px 12px;
  display: flex;
  align-items: center;
  gap: 8px;
  transition: all 0.2s ease;
  min-height: 44px;
}

.input-container:focus-within {
  border-color: rgba(255, 255, 255, 0.25);
  background: rgba(255, 255, 255, 0.1);
}

.textarea {
  flex: 1;
  background: none;
  border: none;
  color: white;
  font-size: 14px;
  line-height: 1.4;
  resize: none;
  outline: none;
  max-height: 100px;
  font-family: inherit;
  padding: 4px 0;
}

.textarea::placeholder {
  color: rgba(255, 255, 255, 0.5);
}

.send-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 20px;
  height: 20px;
  background: rgba(255, 255, 255, 0.6);
  border: none;
  border-radius: 10px;
  color: rgba(0, 0, 0, 0.8);
  cursor: pointer;
  transition: all 0.2s ease;
  flex-shrink: 0;
}

.send-btn:hover:not(:disabled) {
  background: rgba(255, 255, 255, 0.8);
  color: black;
}

.send-btn:disabled {
  opacity: 0.4;
  cursor: not-allowed;
  transform: none;
}

@keyframes fade-in {
  from { opacity: 0; }
  to { opacity: 1; }
}

@keyframes slide-up {
  from { 
    opacity: 0; 
    transform: translateY(10px) scale(0.98); 
  }
  to { 
    opacity: 1; 
    transform: translateY(0) scale(1); 
  }
}

@keyframes fade-slide-in {
  from { 
    opacity: 0; 
    transform: translateY(6px); 
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