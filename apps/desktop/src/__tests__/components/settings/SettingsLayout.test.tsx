import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router";
import { SettingsLayout } from "../../../components/settings/SettingsLayout";

const sections = [
  { id: "account", label: "My Account", content: <div>Account Content</div> },
  { id: "appearance", label: "Appearance", content: <div>Appearance Content</div> },
];

function renderSettingsLayout() {
  return render(
    <MemoryRouter>
      <SettingsLayout sections={sections} />
    </MemoryRouter>,
  );
}

describe("SettingsLayout", () => {
  it("renders navigation sidebar and content area", () => {
    renderSettingsLayout();

    expect(screen.getByRole("navigation")).toBeInTheDocument();
    expect(screen.getByTestId("settings-content")).toBeInTheDocument();
  });

  it("renders all nav items", () => {
    renderSettingsLayout();

    expect(screen.getByText("My Account")).toBeInTheDocument();
    expect(screen.getByText("Appearance")).toBeInTheDocument();
  });

  it("shows first section content by default", () => {
    renderSettingsLayout();

    expect(screen.getByText("Account Content")).toBeInTheDocument();
  });

  it("clicking nav item switches displayed settings section", async () => {
    const user = userEvent.setup();
    renderSettingsLayout();

    await user.click(screen.getByText("Appearance"));

    expect(screen.getByText("Appearance Content")).toBeInTheDocument();
    expect(screen.queryByText("Account Content")).not.toBeInTheDocument();
  });

  it("renders nav footer when provided", () => {
    render(
      <MemoryRouter>
        <SettingsLayout
          sections={sections}
          navFooter={<button>Log Out</button>}
        />
      </MemoryRouter>,
    );

    expect(screen.getByText("Log Out")).toBeInTheDocument();
  });
});
