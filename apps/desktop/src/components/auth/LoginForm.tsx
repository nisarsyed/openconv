import { useNavigate, Link } from "react-router";
import { Button } from "../ui/Button";
import { useAppStore } from "../../store";

export function LoginForm() {
  const login = useAppStore((s) => s.login);
  const isLoading = useAppStore((s) => s.isLoading);
  const error = useAppStore((s) => s.error);
  const navigate = useNavigate();

  const handleLogin = async () => {
    await login();
    if (useAppStore.getState().isAuthenticated) {
      navigate("/app", { replace: true });
    }
  };

  return (
    <div className="flex flex-col gap-5">
      {error && (
        <p role="alert" className="text-xs text-red-400">
          {error}
        </p>
      )}
      <Button
        variant="primary"
        size="lg"
        disabled={isLoading}
        onClick={handleLogin}
        className="w-full"
      >
        {isLoading ? "Logging in..." : "Log In"}
      </Button>
      <div className="flex flex-col gap-2.5 text-center text-sm">
        <Link
          to="/register"
          className="text-[var(--text-link)] transition-all hover:brightness-125"
        >
          Create Account
        </Link>
        <Link
          to="/recover"
          className="text-[var(--text-muted)] transition-colors hover:text-[var(--text-secondary)]"
        >
          Forgot your account?
        </Link>
      </div>
    </div>
  );
}
