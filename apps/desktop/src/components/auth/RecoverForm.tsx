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
      <div className="flex flex-col gap-4 text-center">
        <p className="text-[var(--text-primary)]">
          Check your email for recovery instructions.
        </p>
        <Link to="/login" className="text-[var(--text-link)] hover:underline">
          Back to Login
        </Link>
      </div>
    );
  }

  return (
    <form onSubmit={handleSubmit} noValidate className="flex flex-col gap-4">
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
        <Link to="/login" className="text-[var(--text-link)] hover:underline">
          Back to Login
        </Link>
      </div>
    </form>
  );
}
