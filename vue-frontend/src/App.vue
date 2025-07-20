<script setup>
import { ref, onMounted } from "vue";
import { SparklesIcon } from "@heroicons/vue/24/outline";
import { Motion } from "motion-v";
import AIAssistant from "./components/AIAssistant.vue";

const isVisible = ref(false);
const isAnimating = ref(false);
const isDragging = ref(false);
const isPanelDragging = ref(false);
const buttonPosition = ref({ x: window.innerWidth - 64, y: 20 });
const originalButtonPosition = ref({ x: window.innerWidth - 64, y: 20 });
const dragOffset = ref({ x: 0, y: 0 });
const panelDragOffset = ref({ x: 0, y: 0 });

const toggleAIPanel = () => {
  if (isAnimating.value || isDragging.value) return;

  isAnimating.value = true;
  isVisible.value = !isVisible.value;
  window.aiPanelVisible = isVisible.value;

  if (isVisible.value) {
    // store original position before adjusting
    originalButtonPosition.value = { ...buttonPosition.value };

    // ensure panel stays within bounds
    const panelWidth = Math.min(360, window.innerWidth * 0.28);
    const panelHeight = window.innerHeight - 40;

    // check if panel would overflow right edge
    if (buttonPosition.value.x + panelWidth > window.innerWidth) {
      buttonPosition.value.x = window.innerWidth - panelWidth - 20;
    }

    // check if panel would overflow bottom edge
    if (buttonPosition.value.y + panelHeight > window.innerHeight) {
      buttonPosition.value.y = window.innerHeight - panelHeight - 20;
    }

    // ensure minimum position from left and top edges
    if (buttonPosition.value.x < 20) {
      buttonPosition.value.x = 20;
    }
    if (buttonPosition.value.y < 20) {
      buttonPosition.value.y = 20;
    }

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

  // restore original button position when closing
  buttonPosition.value = { ...originalButtonPosition.value };

  setTimeout(() => {
    isAnimating.value = false;
  }, 350);
};

const startDrag = (event) => {
  if (isVisible.value || isAnimating.value) return;

  isDragging.value = true;
  const rect = event.currentTarget.getBoundingClientRect();
  dragOffset.value = {
    x: event.clientX - rect.left,
    y: event.clientY - rect.top,
  };

  document.addEventListener("mousemove", onDrag);
  document.addEventListener("mouseup", stopDrag);
  event.preventDefault();
};

const onDrag = (event) => {
  if (!isDragging.value) return;

  const newX = event.clientX - dragOffset.value.x;
  const newY = event.clientY - dragOffset.value.y;

  // keep button within viewport bounds
  const maxX = window.innerWidth - 44;
  const maxY = window.innerHeight - 44;

  buttonPosition.value = {
    x: Math.max(0, Math.min(newX, maxX)),
    y: Math.max(0, Math.min(newY, maxY)),
  };
};

const stopDrag = () => {
  isDragging.value = false;
  document.removeEventListener("mousemove", onDrag);
  document.removeEventListener("mouseup", stopDrag);

  // prevent click event from firing after drag
  setTimeout(() => {
    isDragging.value = false;
  }, 10);
};

const startPanelDrag = (event) => {
  if (isAnimating.value || !isVisible.value) return;

  // don't start drag if clicking on input area, close button, or specific message content
  const target = event.target;
  if (
    target.closest(".input-area") ||
    target.closest(".close-corner-btn") ||
    target.closest(".msg") ||
    target.closest(".msg-content") ||
    target.closest(".empty") ||
    target.closest(".typing")
  ) {
    return;
  }

  isPanelDragging.value = true;
  const rect = event.currentTarget.getBoundingClientRect();
  panelDragOffset.value = {
    x: event.clientX - rect.left,
    y: event.clientY - rect.top,
  };

  document.addEventListener("mousemove", onPanelDrag);
  document.addEventListener("mouseup", stopPanelDrag);
  event.preventDefault();
};

const onPanelDrag = (event) => {
  if (!isPanelDragging.value) return;

  const newX = event.clientX - panelDragOffset.value.x;
  const newY = event.clientY - panelDragOffset.value.y;

  // keep panel within viewport bounds
  const panelWidth = Math.min(360, window.innerWidth * 0.28);
  const panelHeight = window.innerHeight - 40;
  const maxX = window.innerWidth - panelWidth;
  const maxY = window.innerHeight - panelHeight;

  buttonPosition.value = {
    x: Math.max(0, Math.min(newX, maxX)),
    y: Math.max(0, Math.min(newY, maxY)),
  };
};

const stopPanelDrag = () => {
  isPanelDragging.value = false;
  document.removeEventListener("mousemove", onPanelDrag);
  document.removeEventListener("mouseup", stopPanelDrag);

  // trigger morph back to normal
  morphBackToNormal();
};

const morphBackToNormal = () => {
  // trigger smooth transition back to normal state
  const container = document.getElementById("ai-container");
  if (container) {
    container.classList.add("morphing-back");
    setTimeout(() => {
      container.classList.remove("morphing-back");
    }, 500);
  }
};

onMounted(() => {
  window.toggleAIPanel = toggleAIPanel;
  window.hideAIPanel = hideAIPanel;

  // reset position when card changes (new session)
  const resetPosition = () => {
    buttonPosition.value = { x: window.innerWidth - 64, y: 20 };
    originalButtonPosition.value = { x: window.innerWidth - 64, y: 20 };
  };

  // listen for card changes
  window.addEventListener("beforeunload", resetPosition);
  window.addEventListener("focus", resetPosition);

  // handle window resize to keep panel in bounds
  window.addEventListener("resize", () => {
    if (isVisible.value) {
      const panelWidth = Math.min(360, window.innerWidth * 0.28);
      const panelHeight = window.innerHeight - 40;

      // adjust position if panel would overflow after resize
      if (buttonPosition.value.x + panelWidth > window.innerWidth) {
        buttonPosition.value.x = window.innerWidth - panelWidth - 20;
      }
      if (buttonPosition.value.y + panelHeight > window.innerHeight) {
        buttonPosition.value.y = window.innerHeight - panelHeight - 20;
      }
      if (buttonPosition.value.x < 20) {
        buttonPosition.value.x = 20;
      }
      if (buttonPosition.value.y < 20) {
        buttonPosition.value.y = 20;
      }
    }
  });
});
</script>

<template>
  <Motion
    id="ai-container"
    :class="{
      expanded: isVisible,
      animating: isAnimating,
      dragging: isDragging || isPanelDragging,
    }"
    class="ai-container"
    :style="{
      left: buttonPosition.x + 'px',
      top: buttonPosition.y + 'px',
    }"
    :transition="{
      type: 'spring',
      stiffness: 600,
      damping: 40,
      mass: 0.4,
      restDelta: 0.001,
      duration: 0.2,
    }"
    :variants="{
      button: {
        scale: 1,
        borderRadius: '22px',
        width: '44px',
        height: '44px',
        background: 'rgba(255, 255, 255, 0.1)',
        backdropFilter: 'blur(16px)',
        boxShadow:
          '0 8px 32px rgba(0, 0, 0, 0.12), 0 2px 8px rgba(0, 0, 0, 0.08), inset 0 1px 0 rgba(255, 255, 255, 0.1)',
      },
      panel: {
        scale: 1,
        borderRadius: '16px',
        width: 'min(360px, 28vw)',
        height: 'calc(100vh - 40px)',
        background: 'rgba(20, 20, 20, 0.8)',
        backdropFilter: 'blur(24px)',
        boxShadow:
          '0 20px 60px rgba(0, 0, 0, 0.3), 0 8px 24px rgba(0, 0, 0, 0.15), inset 0 1px 0 rgba(255, 255, 255, 0.1)',
      },
      dragging: {
        scale: 0.95,
        borderRadius: '20px',
        width: 'min(360px, 28vw)',
        height: 'calc(100vh - 40px)',
        background: 'rgba(255, 255, 255, 0.15)',
        backdropFilter: 'blur(20px)',
        boxShadow:
          '0 20px 60px rgba(0, 0, 0, 0.4), 0 8px 24px rgba(0, 0, 0, 0.2), inset 0 1px 0 rgba(255, 255, 255, 0.2)',
      },
    }"
    :initial="'button'"
    :animate="
      isVisible
        ? isDragging || isPanelDragging
          ? 'dragging'
          : 'panel'
        : 'button'
    "
  >
    <div
      v-if="!isVisible"
      class="ai-button-content"
      @click="toggleAIPanel"
      @mousedown="startDrag"
    >
      <SparklesIcon class="ai-icon" />
    </div>

    <div
      v-if="isVisible"
      class="ai-panel-content"
      @mousedown="startPanelDrag"
    >
      <AIAssistant @close="hideAIPanel" />
    </div>
  </Motion>
</template>

<style scoped>
.ai-container {
  position: fixed;
  z-index: 1001;
  pointer-events: auto;
  font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;

  width: 44px;
  height: 44px;
  background: rgba(255, 255, 255, 0.1);
  backdrop-filter: blur(16px);
  border: 1px solid rgba(255, 255, 255, 0.2);
  border-radius: 22px;
  transition: all 0.35s cubic-bezier(0.4, 0, 0.2, 1);
  box-shadow: 0 8px 32px rgba(0, 0, 0, 0.12), 0 2px 8px rgba(0, 0, 0, 0.08),
    inset 0 1px 0 rgba(255, 255, 255, 0.1);
  cursor: pointer;
  overflow: hidden;

  /* GPU acceleration */
  transform: translateZ(0);
  will-change: transform, backdrop-filter;
  backface-visibility: hidden;
}

.ai-container:not(.expanded):hover:not(.dragging) {
  background: rgba(255, 255, 255, 0.15);
  transform: scale(1.05);
  box-shadow: 0 12px 40px rgba(0, 0, 0, 0.15), 0 4px 12px rgba(0, 0, 0, 0.1),
    inset 0 1px 0 rgba(255, 255, 255, 0.15);
}

.ai-container.dragging {
  transition: none;
  cursor: grabbing;
  transform: scale(1.1);
  box-shadow: 0 16px 48px rgba(0, 0, 0, 0.2), 0 6px 16px rgba(0, 0, 0, 0.15),
    inset 0 1px 0 rgba(255, 255, 255, 0.2);
}

.ai-container.expanded {
  cursor: grab;
}

.ai-container.expanded:active {
  cursor: grabbing;
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
  cursor: grab;
  user-select: none;
}

.ai-container:not(.expanded):hover:not(.dragging) .ai-button-content {
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
