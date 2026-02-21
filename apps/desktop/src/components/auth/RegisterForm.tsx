import { useState } from "react";
import { useNavigate, Link } from "react-router";
import { Input } from "../ui/Input";
import { Button } from "../ui/Button";
import { useAppStore } from "../../store";

const EMAIL_REGEX = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;

export function RegisterForm() {
  const [email, setEmail] = useState("");
  const [displayName, setDisplayName] = useState("");
  const [code, setCode] = useState("");
  const [errors, setErrors] = useState<{
    email?: string;
    displayName?: string;
    code?: string;
  }>({});

  const registrationStep = useAppStore((s) => s.registrationStep);
  const isLoading = useAppStore((s) => s.isLoading);
  const serverError = useAppStore((s) => s.error);
  const registerStart = useAppStore((s) => s.registerStart);
  const registerVerify = useAppStore((s) => s.registerVerify);
  const registerComplete = useAppStore((s) => s.registerComplete);
  const clearError = useAppStore((s) => s.clearError);
  const navigate = useNavigate();

  const validateStart = (): boolean => {
    const newErrors: { email?: string; displayName?: string } = {};
    if (!EMAIL_REGEX.test(email)) {
      newErrors.email = "Please enter a valid email address";
    }
    if (displayName.length < 2) {
      newErrors.displayName = "Display name must be at least 2 characters";
    }
    setErrors(newErrors);
    return Object.keys(newErrors).length === 0;
  };

  const handleStart = async (e: React.FormEvent) => {
    e.preventDefault();
    clearError();
    if (!validateStart()) return;
    await registerStart(email, displayName);
  };

  const handleVerify = async (e: React.FormEvent) => {
    e.preventDefault();
    clearError();
    if (code.trim().length < 6) {
      setErrors({ code: "Please enter the 6-digit code" });
      return;
    }
    setErrors({});
    await registerVerify(email, code);
    // Auto-advance to complete step
    if (useAppStore.getState().registrationStep === "verified") {
      await registerComplete(displayName);
      if (useAppStore.getState().isAuthenticated) {
        navigate("/app", { replace: true });
      }
    }
  };

  if (registrationStep === "email_sent") {
    return (
      <form onSubmit={handleVerify} noValidate className="flex flex-col gap-5">
        <div className="text-center text-sm text-[var(--text-secondary)]">
          We sent a verification code to <strong>{email}</strong>
        </div>
        <Input
          label="Verification Code"
          type="text"
          value={code}
          onChange={(e) => setCode(e.target.value)}
          placeholder="123456"
          error={errors.code}
        />
        {serverError && (
          <p role="alert" className="-mt-2 text-xs text-red-400">
            {serverError}
          </p>
        )}
        <Button
          type="submit"
          variant="primary"
          size="lg"
          disabled={!code.trim() || isLoading}
          className="w-full"
        >
          {isLoading ? "Verifying..." : "Verify & Create Account"}
        </Button>
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
        error={errors.email}
      />
      <Input
        label="Display Name"
        type="text"
        value={displayName}
        onChange={(e) => setDisplayName(e.target.value)}
        placeholder="Your display name"
        error={errors.displayName}
      />
      {serverError && (
        <p role="alert" className="-mt-2 text-xs text-red-400">
          {serverError}
        </p>
      )}
      <Button
        type="submit"
        variant="primary"
        size="lg"
        disabled={!email.trim() || !displayName.trim() || isLoading}
        className="w-full"
      >
        {isLoading ? "Sending..." : "Create Account"}
      </Button>
      <div className="text-center text-sm">
        <span className="text-[var(--text-muted)]">
          Already have an account?{" "}
        </span>
        <Link
          to="/login"
          className="text-[var(--text-link)] transition-all hover:brightness-125"
        >
          Log In
        </Link>
      </div>
    </form>
  );
}
