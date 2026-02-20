import { render, type RenderOptions } from "@testing-library/react";
import { MemoryRouter, type MemoryRouterProps } from "react-router";
import { useAppStore } from "../../store";
import type { AppStore } from "../../store";
import { seedStores } from "../../mock/seed";

interface RenderWithProvidersOptions extends Omit<RenderOptions, "wrapper"> {
  initialEntries?: MemoryRouterProps["initialEntries"];
  storeOverrides?: Partial<AppStore>;
  seed?: boolean;
}

export function renderWithProviders(
  ui: React.ReactElement,
  {
    initialEntries = ["/app"],
    storeOverrides,
    seed = true,
    ...renderOptions
  }: RenderWithProvidersOptions = {},
) {
  // Reset store to defaults
  useAppStore.setState(useAppStore.getInitialState(), true);

  // Optionally seed with mock data
  if (seed) {
    seedStores();
  }

  // Apply any overrides on top
  if (storeOverrides) {
    useAppStore.setState(storeOverrides);
  }

  function Wrapper({ children }: { children: React.ReactNode }) {
    return (
      <MemoryRouter initialEntries={initialEntries}>{children}</MemoryRouter>
    );
  }

  return {
    ...render(ui, { wrapper: Wrapper, ...renderOptions }),
    store: useAppStore,
  };
}
