import { AuthPageLayout } from "../components/auth/AuthPageLayout";
import { LoginForm } from "../components/auth/LoginForm";

export function LoginPage() {
  return (
    <AuthPageLayout subtitle="Welcome back!">
      <LoginForm />
    </AuthPageLayout>
  );
}
