import { useState } from "react";
import { Link } from "react-router";
import { Input } from "../ui/Input";
import { Button } from "../ui/Button";

export function RecoverForm() {
  const [email, setEmail] = useState("");
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [submitted, setSubmitted] = useState(false);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setIsSubmitting(true);
    await new Promise((r) => setTimeout(r, 500));
    setSubmitted(true);
    setIsSubmitting(false);
  };

  if (submitted) {
    return (
      <div className="animate-fade-in flex flex-col gap-5 text-center">
        <div className="mx-auto flex h-12 w-12 items-center justify-center rounded-full bg-[var(--bg-accent-subtle)]">
          <svg
            className="h-6 w-6 text-[var(--bg-accent)]"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth={2}
          >
            <path d="M3 8l7.89 5.26a2 2 0 002.22 0L21 8M5 19h14a2 2 0 002-2V7a2 2 0 00-2-2H5a2 2 0 00-2 2v10a2 2 0 002 2z" />
          </svg>
        </div>
        <p className="text-sm text-[var(--text-primary)]">
          Check your email for recovery instructions.
        </p>
        <Link
          to="/login"
          className="text-sm text-[var(--text-link)] transition-all hover:brightness-125"
        >
          Back to Login
        </Link>
      </div>
    );
  }

  return (
    <form onSubmit={handleSubmit} noValidate className="flex flex-col gap-5">
      <Input
        label="Email"
        type="email"
        value={email}
        onChange={(e) => setEmail(e.target.value)}
        placeholder="you@example.com"
      />
      <Button
        type="submit"
        variant="primary"
        size="lg"
        disabled={!email.trim() || isSubmitting}
        className="w-full"
      >
        {isSubmitting ? "Sending..." : "Send Recovery Email"}
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
