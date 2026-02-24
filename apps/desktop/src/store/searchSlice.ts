import type { SearchResult, SearchScope } from "../bindings";
import type { SliceCreator } from "./index";
import { commands } from "../bindings";

export interface SearchSlice {
  searchQuery: string;
  searchScope: SearchScope;
  searchResults: SearchResult[];
  isSearching: boolean;
  searchOverlayVisible: boolean;

  setSearchQuery: (query: string) => void;
  setSearchScope: (scope: SearchScope) => void;
  performSearch: () => Promise<void>;
  clearSearch: () => void;
  setSearchOverlayVisible: (visible: boolean) => void;
}

export const createSearchSlice: SliceCreator<SearchSlice> = (set, get) => ({
  searchQuery: "",
  searchScope: "AllMessages" as SearchScope,
  searchResults: [],
  isSearching: false,
  searchOverlayVisible: false,

  setSearchQuery: (query) =>
    set((draft) => {
      draft.searchQuery = query;
    }),

  setSearchScope: (scope) =>
    set((draft) => {
      draft.searchScope = scope;
    }),

  performSearch: async () => {
    const { searchQuery, searchScope } = get();
    if (!searchQuery.trim()) {
      set((draft) => {
        draft.searchResults = [];
        draft.isSearching = false;
      });
      return;
    }

    set((draft) => {
      draft.isSearching = true;
    });

    const result = await commands.searchMessages(
      searchQuery,
      searchScope,
      50,
    );
    if (result.status === "ok") {
      set((draft) => {
        draft.searchResults = result.data;
        draft.isSearching = false;
      });
    } else {
      console.error("Search failed:", result.error);
      set((draft) => {
        draft.searchResults = [];
        draft.isSearching = false;
      });
    }
  },

  clearSearch: () =>
    set((draft) => {
      draft.searchQuery = "";
      draft.searchResults = [];
      draft.isSearching = false;
      draft.searchOverlayVisible = false;
    }),

  setSearchOverlayVisible: (visible) =>
    set((draft) => {
      draft.searchOverlayVisible = visible;
    }),
});
