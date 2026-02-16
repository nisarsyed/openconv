import { useState } from "react";
import { useNavigate, Link } from "react-router";
import { Input } from "../ui/Input";
import { Button } from "../ui/Button";
import { useAppStore } from "../../store";
import { mockLogin } from "../../mock/api";

export function LoginForm() {
  const [email, setEmail] = useState("");
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const login = useAppStore((state) => state.login);
  const navigate = useNavigate();

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setError(null);
    setIsSubmitting(true);
    try {
      const result = await mockLogin(email);
      login(result.user, result.keyPair, result.token);
      navigate("/app", { replace: true });
    } catch (err) {
      setError(err instanceof Error ? err.message : "Login failed");
      setIsSubmitting(false);
    }
  };

  return (
    <form onSubmit={handleSubmit} noValidate className="flex flex-col gap-4">
      <Input
        label="Email"
        type="email"
        value={email}
        onChange={(e) => setEmail(e.target.value)}
        placeholder="you@example.com"
      />
      {error && (
        <p role="alert" className="text-xs text-red-400">{error}</p>
      )}
      <Button
        type="submit"
        variant="primary"
        size="lg"
        disabled={!email.trim() || isSubmitting}
        className="w-full"
      >
        {isSubmitting ? "Logging in..." : "Log In"}
      </Button>
      <div className="flex flex-col gap-2 text-center text-sm">
        <Link to="/register" className="text-[var(--text-link)] hover:underline">
          Create Account
        </Link>
        <Link to="/recover" className="text-[var(--text-link)] hover:underline">
          Forgot your account?
        </Link>
      </div>
    </form>
  );
}
