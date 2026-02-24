import { useEffect, useRef } from "react";
import { useAppStore } from "../store";

export function useSearch() {
  const query = useAppStore((s) => s.searchQuery);
  const scope = useAppStore((s) => s.searchScope);
  const results = useAppStore((s) => s.searchResults);
  const isSearching = useAppStore((s) => s.isSearching);
  const setQuery = useAppStore((s) => s.setSearchQuery);
  const setScope = useAppStore((s) => s.setSearchScope);
  const performSearch = useAppStore((s) => s.performSearch);
  const clear = useAppStore((s) => s.clearSearch);

  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    if (timerRef.current) {
      clearTimeout(timerRef.current);
    }

    if (!query.trim()) {
      // Clear results but keep overlay open
      useAppStore.setState((draft) => {
        draft.searchResults = [];
        draft.isSearching = false;
      });
      return;
    }

    timerRef.current = setTimeout(() => {
      performSearch();
    }, 300);

    return () => {
      if (timerRef.current) {
        clearTimeout(timerRef.current);
      }
    };
  }, [query, scope, performSearch]);

  return { query, setQuery, scope, setScope, results, isSearching, clear };
}
