<script setup>
import { ref, onMounted } from "vue";
import { SparklesIcon } from "@heroicons/vue/24/outline";
import AIAssistant from "./components/AIAssistant.vue";

const isVisible = ref(false);
const isAnimating = ref(false);

const toggleAIPanel = () => {
  if (isAnimating.value) return;

  isAnimating.value = true;
  isVisible.value = !isVisible.value;
  window.aiPanelVisible = isVisible.value;

  if (isVisible.value) {
    setTimeout(() => {
      window.dispatchEvent(new CustomEvent("ai-panel-opened"));
    }, 150);
  }

  setTimeout(() => {
    isAnimating.value = false;
  }, 350);
};

const hideAIPanel = () => {
  if (isAnimating.value) return;

  isAnimating.value = true;
  isVisible.value = false;
  window.aiPanelVisible = false;

  setTimeout(() => {
    isAnimating.value = false;
  }, 350);
};

onMounted(() => {
  window.toggleAIPanel = toggleAIPanel;
  window.hideAIPanel = hideAIPanel;
});
</script>

<template>
  <div
    id="ai-container"
    :class="{
      expanded: isVisible,
      animating: isAnimating,
    }"
    class="ai-container"
  >
    <div
      v-if="!isVisible"
      class="ai-button-content"
      @click="toggleAIPanel"
    >
      <SparklesIcon class="ai-icon" />
    </div>

    <div
      v-if="isVisible"
      class="ai-panel-content"
    >
      <AIAssistant @close="hideAIPanel" />
    </div>
  </div>
</template>

<style scoped>
.ai-container {
  position: fixed;
  top: 20px;
  right: 20px;
  z-index: 1001;
  pointer-events: auto;
  font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;

  width: 44px;
  height: 44px;
  background: rgba(255, 255, 255, 0.1);
  backdrop-filter: blur(20px) saturate(180%);
  border: 1px solid rgba(255, 255, 255, 0.2);
  border-radius: 22px;
  transition: all 0.35s cubic-bezier(0.4, 0, 0.2, 1);
  box-shadow: 0 8px 32px rgba(0, 0, 0, 0.12), 0 2px 8px rgba(0, 0, 0, 0.08),
    inset 0 1px 0 rgba(255, 255, 255, 0.1);
  cursor: pointer;
  overflow: hidden;
}

.ai-container:not(.expanded):hover {
  background: rgba(255, 255, 255, 0.15);
  transform: scale(1.05);
  box-shadow: 0 12px 40px rgba(0, 0, 0, 0.15), 0 4px 12px rgba(0, 0, 0, 0.1),
    inset 0 1px 0 rgba(255, 255, 255, 0.15);
}

.ai-container.expanded {
  width: min(360px, 28vw);
  height: calc(100vh - 40px);
  border-radius: 16px;
  cursor: default;
  background: rgba(20, 20, 20, 0.8);
  backdrop-filter: blur(40px) saturate(180%);
  border: 1px solid rgba(255, 255, 255, 0.1);

  box-shadow: 0 20px 60px rgba(0, 0, 0, 0.3), 0 8px 24px rgba(0, 0, 0, 0.15),
    inset 0 1px 0 rgba(255, 255, 255, 0.1);
}

.ai-button-content {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 100%;
  height: 100%;
  color: rgba(255, 255, 255, 0.8);
  transition: all 0.3s ease;
  cursor: pointer;
}

.ai-container:not(.expanded):hover .ai-button-content {
  color: rgba(255, 255, 255, 0.95);
  transform: scale(1.1);
}

.ai-icon {
  width: 20px;
  height: 20px;
}

.ai-panel-content {
  display: flex;
  flex-direction: column;
  width: 100%;
  height: 100%;
  opacity: 0;
  animation: fadeInContent 0.4s cubic-bezier(0.4, 0, 0.2, 1) 0.1s forwards;
}

@keyframes fadeInContent {
  from {
    opacity: 0;
    transform: translateY(8px);
  }
  to {
    opacity: 1;
    transform: translateY(0);
  }
}

.ai-container.expanded {
  overflow-y: auto;
}

.ai-container.expanded::-webkit-scrollbar {
  width: 3px;
}

.ai-container.expanded::-webkit-scrollbar-track {
  background: transparent;
}

.ai-container.expanded::-webkit-scrollbar-thumb {
  background: rgba(255, 255, 255, 0.08);
  border-radius: 1.5px;
}

.ai-container.expanded::-webkit-scrollbar-thumb:hover {
  background: rgba(255, 255, 255, 0.15);
}
</style>
