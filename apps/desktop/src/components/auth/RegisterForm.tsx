import { useState } from "react";
import { useNavigate, Link } from "react-router";
import { Input } from "../ui/Input";
import { Button } from "../ui/Button";
import { useAppStore } from "../../store";
import { mockRegister } from "../../mock/api";

const EMAIL_REGEX = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;

export function RegisterForm() {
  const [email, setEmail] = useState("");
  const [displayName, setDisplayName] = useState("");
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [errors, setErrors] = useState<{ email?: string; displayName?: string }>({});
  const [serverError, setServerError] = useState<string | null>(null);
  const login = useAppStore((state) => state.login);
  const navigate = useNavigate();

  const validate = (): boolean => {
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

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setServerError(null);
    if (!validate()) return;
    setIsSubmitting(true);
    try {
      const result = await mockRegister(email, displayName);
      login(result.user, result.keyPair, result.token);
      navigate("/app", { replace: true });
    } catch (err) {
      setServerError(err instanceof Error ? err.message : "Registration failed");
      setIsSubmitting(false);
    }
  };

  return (
    <form onSubmit={handleSubmit} noValidate className="flex flex-col gap-5">
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
        <p role="alert" className="text-xs text-red-400 -mt-2">{serverError}</p>
      )}
      <Button
        type="submit"
        variant="primary"
        size="lg"
        disabled={!email.trim() || !displayName.trim() || isSubmitting}
        className="w-full"
      >
        {isSubmitting ? "Creating Account..." : "Create Account"}
      </Button>
      <div className="text-center text-sm">
        <span className="text-[var(--text-muted)]">Already have an account? </span>
        <Link to="/login" className="text-[var(--text-link)] hover:brightness-125 transition-all">
          Log In
        </Link>
      </div>
    </form>
  );
}
