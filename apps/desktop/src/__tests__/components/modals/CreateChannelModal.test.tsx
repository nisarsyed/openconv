import { describe, it, expect } from "vitest";
import { screen, fireEvent } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { renderWithProviders } from "../../helpers/renderWithProviders";
import { CreateChannelModal } from "../../../components/modals/CreateChannelModal";
import { useAppStore } from "../../../store";
import { mockGuilds } from "../../../mock/data";

const guildId = mockGuilds[0].id;

function renderModal() {
  return renderWithProviders(<CreateChannelModal guildId={guildId} />);
}

describe("CreateChannelModal", () => {
  it("renders name input, type select, and category select", () => {
    renderModal();

    expect(screen.getByLabelText(/channel name/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/channel type/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/category/i)).toBeInTheDocument();
  });

  it("create button calls createChannel with correct arguments", async () => {
    const user = userEvent.setup();
    renderModal();

    const initialChannelCount = (
      useAppStore.getState().channelIdsByGuild[guildId] ?? []
    ).length;

    await user.type(screen.getByLabelText(/channel name/i), "new-channel");
    await user.click(screen.getByRole("button", { name: /create/i }));

    const channelIds = useAppStore.getState().channelIdsByGuild[guildId] ?? [];
    expect(channelIds.length).toBe(initialChannelCount + 1);

    const newChannelId = channelIds[channelIds.length - 1];
    const newChannel = useAppStore.getState().channelsById[newChannelId];
    expect(newChannel.name).toBe("new-channel");
    expect(newChannel.guildId).toBe(guildId);
    expect(newChannel.channelType).toBe("text");
  });

  it("closes modal after successful creation", async () => {
    const user = userEvent.setup();
    renderModal();

    useAppStore.setState({
      activeModal: { type: "createChannel", props: { guildId } },
    });

    await user.type(screen.getByLabelText(/channel name/i), "test-channel");
    await user.click(screen.getByRole("button", { name: /create/i }));

    expect(useAppStore.getState().activeModal).toBeNull();
  });

  it("create button is disabled when name is empty", () => {
    renderModal();

    expect(screen.getByRole("button", { name: /create/i })).toBeDisabled();
  });

  it("auto-lowercases and replaces spaces with hyphens", () => {
    renderModal();

    const input = screen.getByLabelText(/channel name/i);
    fireEvent.change(input, { target: { value: "My New Channel" } });

    expect(input).toHaveValue("my-new-channel");
  });
});
