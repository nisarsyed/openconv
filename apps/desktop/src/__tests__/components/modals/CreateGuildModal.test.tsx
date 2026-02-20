import { describe, it, expect } from "vitest";
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { renderWithProviders } from "../../helpers/renderWithProviders";
import { CreateGuildModal } from "../../../components/modals/CreateGuildModal";
import { useAppStore } from "../../../store";

function renderModal() {
  return renderWithProviders(<CreateGuildModal />);
}

describe("CreateGuildModal", () => {
  it("renders name input and create button", () => {
    renderModal();

    expect(screen.getByLabelText(/server name/i)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /create/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /cancel/i })).toBeInTheDocument();
  });

  it("create button is disabled when name is empty", () => {
    renderModal();

    expect(screen.getByRole("button", { name: /create/i })).toBeDisabled();
  });

  it("create button calls createGuild with input value", async () => {
    const user = userEvent.setup();
    renderModal();

    const initialGuildCount = useAppStore.getState().guildIds.length;

    await user.type(screen.getByLabelText(/server name/i), "My New Server");
    await user.click(screen.getByRole("button", { name: /create/i }));

    expect(useAppStore.getState().guildIds.length).toBe(initialGuildCount + 1);
    const newGuildId =
      useAppStore.getState().guildIds[
        useAppStore.getState().guildIds.length - 1
      ];
    expect(useAppStore.getState().guildsById[newGuildId].name).toBe(
      "My New Server",
    );
  });

  it("closes modal after successful creation", async () => {
    const user = userEvent.setup();
    renderModal();

    useAppStore.setState({ activeModal: { type: "createGuild" } });

    await user.type(screen.getByLabelText(/server name/i), "Test Guild");
    await user.click(screen.getByRole("button", { name: /create/i }));

    expect(useAppStore.getState().activeModal).toBeNull();
  });

  it("cancel button closes modal without creating", async () => {
    const user = userEvent.setup();
    renderModal();

    useAppStore.setState({ activeModal: { type: "createGuild" } });
    const initialGuildCount = useAppStore.getState().guildIds.length;

    await user.click(screen.getByRole("button", { name: /cancel/i }));

    expect(useAppStore.getState().activeModal).toBeNull();
    expect(useAppStore.getState().guildIds.length).toBe(initialGuildCount);
  });
});
