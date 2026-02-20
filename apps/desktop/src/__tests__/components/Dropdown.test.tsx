import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, it, expect, vi } from "vitest";
import { Dropdown } from "../../components/ui/Dropdown";

const items = [
  { id: "edit", label: "Edit" },
  { id: "delete", label: "Delete", danger: true },
];

describe("Dropdown", () => {
  it("opens menu on trigger click", async () => {
    render(
      <Dropdown
        trigger={<button>Actions</button>}
        items={items}
        onSelect={() => {}}
      />,
    );
    expect(screen.queryByRole("list")).not.toBeInTheDocument();
    await userEvent.click(screen.getByText("Actions"));
    expect(screen.getByRole("list")).toBeInTheDocument();
  });

  it("closes menu on outside click", async () => {
    render(
      <div>
        <Dropdown
          trigger={<button>Actions</button>}
          items={items}
          onSelect={() => {}}
        />
        <button>Outside</button>
      </div>,
    );
    await userEvent.click(screen.getByText("Actions"));
    expect(screen.getByRole("list")).toBeInTheDocument();
    await userEvent.click(screen.getByText("Outside"));
    expect(screen.queryByRole("list")).not.toBeInTheDocument();
  });

  it("calls onSelect for selected item", async () => {
    const onSelect = vi.fn();
    render(
      <Dropdown
        trigger={<button>Actions</button>}
        items={items}
        onSelect={onSelect}
      />,
    );
    await userEvent.click(screen.getByText("Actions"));
    await userEvent.click(screen.getByText("Edit"));
    expect(onSelect).toHaveBeenCalledWith("edit");
  });

  it("closes after selection", async () => {
    render(
      <Dropdown
        trigger={<button>Actions</button>}
        items={items}
        onSelect={() => {}}
      />,
    );
    await userEvent.click(screen.getByText("Actions"));
    await userEvent.click(screen.getByText("Edit"));
    expect(screen.queryByRole("list")).not.toBeInTheDocument();
  });
});
