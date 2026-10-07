import { useLayoutEffect, useRef } from "react";
import { api } from "./api";

/** Size the native window to the rendered content, then show it. */
export function useFitWindow(deps: unknown[]) {
  const ref = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const fit = () => api.ready(Math.ceil(el.getBoundingClientRect().height)).catch(() => {});
    fit();
    const ro = new ResizeObserver(fit);
    ro.observe(el);
    return () => ro.disconnect();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, deps);
  return ref;
}
