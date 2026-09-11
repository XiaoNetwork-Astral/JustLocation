// 测试环境的公共桩。
//
// jsdom 没有实现 ResizeObserver，而地图组件用它跟随容器尺寸变化。
// 缺了它会在渲染阶段直接抛错，掩盖真正的断言失败。
class ResizeObserverStub {
  observe() {}
  unobserve() {}
  disconnect() {}
}

if (!('ResizeObserver' in globalThis)) {
  (globalThis as unknown as { ResizeObserver: unknown }).ResizeObserver = ResizeObserverStub;
}

// 地图标签用 dialog.showModal；jsdom 同样没有，组件已按能力判断回退到 open 属性。
if (typeof HTMLDialogElement !== 'undefined' && typeof HTMLDialogElement.prototype.showModal !== 'function') {
  HTMLDialogElement.prototype.showModal = function showModal() { this.open = true; };
  HTMLDialogElement.prototype.close = function close() { this.open = false; };
}
