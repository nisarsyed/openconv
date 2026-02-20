import { describe, it, expect, vi } from "vitest";
import { screen, fireEvent } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { renderWithProviders } from "../../helpers/renderWithProviders";
import { ConfirmDialog } from "../../../components/modals/ConfirmDialog";
import { useAppStore } from "../../../store";

const defaultProps = {
  title: "Delete Message",
  message: "Are you sure you want to delete this message?",
  onConfirm: vi.fn(),
};

function renderDialog(props = defaultProps) {
  useAppStore.setState({
    activeModal: {
      type: "confirm",
      props: {
        title: props.title,
        message: props.message,
      },
    },
  });
  return renderWithProviders(<ConfirmDialog {...props} />);
}

describe("ConfirmDialog", () => {
  it("renders title and message", () => {
    renderDialog();

    expect(screen.getByText("Delete Message")).toBeInTheDocument();
    expect(
      screen.getByText("Are you sure you want to delete this message?"),
    ).toBeInTheDocument();
  });

  it("confirm button calls onConfirm callback", async () => {
    const onConfirm = vi.fn();
    const user = userEvent.setup();
    renderDialog({ ...defaultProps, onConfirm });

    await user.click(screen.getByRole("button", { name: /confirm/i }));

    expect(onConfirm).toHaveBeenCalledOnce();
  });

  it("confirm button has danger/red styling", () => {
    renderDialog();

    const confirmBtn = screen.getByRole("button", { name: /confirm/i });
    expect(confirmBtn.className).toMatch(/red|danger/);
  });

  it("cancel button closes dialog without confirming", async () => {
    const onConfirm = vi.fn();
    const user = userEvent.setup();
    renderDialog({ ...defaultProps, onConfirm });

    useAppStore.setState({
      activeModal: { type: "confirm", props: {} },
    });

    await user.click(screen.getByRole("button", { name: /cancel/i }));

    expect(useAppStore.getState().activeModal).toBeNull();
    expect(onConfirm).not.toHaveBeenCalled();
  });

  it("Escape key closes dialog without confirming", () => {
    const onConfirm = vi.fn();
    renderDialog({ ...defaultProps, onConfirm });

    useAppStore.setState({
      activeModal: { type: "confirm", props: {} },
    });

    fireEvent.keyDown(document, { key: "Escape" });

    expect(useAppStore.getState().activeModal).toBeNull();
    expect(onConfirm).not.toHaveBeenCalled();
  });
});
