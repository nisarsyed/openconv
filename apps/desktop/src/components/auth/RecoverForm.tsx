import { useState } from "react";
import { useNavigate, Link } from "react-router";
import { Input } from "../ui/Input";
import { Button } from "../ui/Button";
import { useAppStore } from "../../store";

export function RecoverForm() {
  const [email, setEmail] = useState("");
  const [code, setCode] = useState("");

  const recoveryStep = useAppStore((s) => s.recoveryStep);
  const isLoading = useAppStore((s) => s.isLoading);
  const error = useAppStore((s) => s.error);
  const recoverStart = useAppStore((s) => s.recoverStart);
  const recoverVerify = useAppStore((s) => s.recoverVerify);
  const recoverComplete = useAppStore((s) => s.recoverComplete);
  const clearError = useAppStore((s) => s.clearError);
  const navigate = useNavigate();

  const handleStart = async (e: React.FormEvent) => {
    e.preventDefault();
    clearError();
    await recoverStart(email);
  };

  const handleVerify = async (e: React.FormEvent) => {
    e.preventDefault();
    clearError();
    await recoverVerify(email, code);
    // Auto-advance to complete step
    if (useAppStore.getState().recoveryStep === "verified") {
      await recoverComplete();
      if (useAppStore.getState().isAuthenticated) {
        navigate("/app", { replace: true });
      }
    }
  };

  if (recoveryStep === "email_sent") {
    return (
      <form onSubmit={handleVerify} noValidate className="flex flex-col gap-5">
        <div className="text-center text-sm text-[var(--text-secondary)]">
          We sent a recovery code to <strong>{email}</strong>
        </div>
        <Input
          label="Recovery Code"
          type="text"
          value={code}
          onChange={(e) => setCode(e.target.value)}
          placeholder="123456"
        />
        {error && (
          <p role="alert" className="-mt-2 text-xs text-red-400">
            {error}
          </p>
        )}
        <Button
          type="submit"
          variant="primary"
          size="lg"
          disabled={!code.trim() || isLoading}
          className="w-full"
        >
          {isLoading ? "Recovering..." : "Verify & Recover Account"}
        </Button>
        <div className="text-center text-sm">
          <Link
            to="/login"
            className="text-[var(--text-muted)] transition-colors hover:text-[var(--text-secondary)]"
          >
            Back to Login
          </Link>
        </div>
      </form>
    );
  }

  return (
    <form onSubmit={handleStart} noValidate className="flex flex-col gap-5">
      <Input
        label="Email"
        type="email"
        value={email}
        onChange={(e) => setEmail(e.target.value)}
        placeholder="you@example.com"
      />
      {error && (
        <p role="alert" className="-mt-2 text-xs text-red-400">
          {error}
        </p>
      )}
      <Button
        type="submit"
        variant="primary"
        size="lg"
        disabled={!email.trim() || isLoading}
        className="w-full"
      >
        {isLoading ? "Sending..." : "Send Recovery Email"}
      </Button>
      <div className="text-center text-sm">
        <Link
          to="/login"
          className="text-[var(--text-muted)] transition-colors hover:text-[var(--text-secondary)]"
        >
          Back to Login
        </Link>
      </div>
    </form>
  );
}
