import { useState } from "react";
import { useNavigate } from "react-router";
import { useAppStore } from "../../store";
import { SettingsLayout } from "./SettingsLayout";
import { AppearanceSettings } from "./AppearanceSettings";
import { Input } from "../ui/Input";
import { Button } from "../ui/Button";
import { Avatar } from "../ui/Avatar";

function AccountSection() {
  const currentUser = useAppStore((s) => s.currentUser);
  const updateProfile = useAppStore((s) => s.updateProfile);

  const [displayName, setDisplayName] = useState(
    currentUser?.displayName ?? "",
  );
  const [email] = useState(currentUser?.email ?? "");

  if (!currentUser) return null;

  const hasChanges = displayName !== currentUser.displayName;
  const nameError =
    displayName.length < 2 ? "Display name must be at least 2 characters" : "";

  function handleSave() {
    if (nameError || !hasChanges) return;
    updateProfile({ displayName });
    setDisplayName(displayName.trim());
  }

  return (
    <div>
      <h2 className="mb-6 text-xl font-bold tracking-[-0.02em] text-[var(--text-primary)]">
        My Account
      </h2>

      <div className="mb-8 flex items-center gap-4">
        <Avatar
          src={currentUser.avatarUrl}
          name={displayName || currentUser.displayName}
          size="lg"
        />
        <div className="text-sm text-[var(--text-muted)]">
          Avatar editing coming soon
        </div>
      </div>

      <div className="max-w-md space-y-5">
        <Input
          label="Display Name"
          value={displayName}
          onChange={(e) => setDisplayName(e.target.value)}
          error={
            displayName.length > 0 && displayName.length < 2
              ? nameError
              : undefined
          }
        />

        <Input label="Email" value={email} readOnly />

        <Button onClick={handleSave} disabled={!hasChanges || !!nameError}>
          Save Changes
        </Button>
      </div>
    </div>
  );
}

export function UserSettings() {
  const navigate = useNavigate();
  const logout = useAppStore((s) => s.logout);

  function handleLogout() {
    logout();
    navigate("/login");
  }

  const sections = [
    { id: "account", label: "My Account", content: <AccountSection /> },
    { id: "appearance", label: "Appearance", content: <AppearanceSettings /> },
  ];

  return (
    <SettingsLayout
      sections={sections}
      navFooter={
        <Button
          variant="danger"
          className="w-full"
          onClick={handleLogout}
          aria-label="Log Out"
        >
          Log Out
        </Button>
      }
    />
  );
}
