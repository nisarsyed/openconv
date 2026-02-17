import { useEffect, useRef } from "react";
import { useAppStore } from "../store";

const BREAKPOINT = 800;
const DEBOUNCE_MS = 150;

export function useResponsiveCollapse() {
  const timerRef = useRef<ReturnType<typeof setTimeout>>();

  useEffect(() => {
    const handleResize = () => {
      if (timerRef.current) clearTimeout(timerRef.current);
      timerRef.current = setTimeout(() => {
        if (window.innerWidth < BREAKPOINT) {
          useAppStore.getState().setMemberListVisible(false);
        }
      }, DEBOUNCE_MS);
    };

    // Check on mount
    if (window.innerWidth < BREAKPOINT) {
      useAppStore.getState().setMemberListVisible(false);
    }

    window.addEventListener("resize", handleResize);
    return () => {
      window.removeEventListener("resize", handleResize);
      if (timerRef.current) clearTimeout(timerRef.current);
    };
  }, []);
}
