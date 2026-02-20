import { AuthPageLayout } from "../components/auth/AuthPageLayout";
import { RegisterForm } from "../components/auth/RegisterForm";

export function RegisterPage() {
  return (
    <AuthPageLayout subtitle="Create an account">
      <RegisterForm />
    </AuthPageLayout>
  );
}
