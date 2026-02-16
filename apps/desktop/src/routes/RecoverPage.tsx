import { AuthPageLayout } from "../components/auth/AuthPageLayout";
import { RecoverForm } from "../components/auth/RecoverForm";

export function RecoverPage() {
  return (
    <AuthPageLayout subtitle="Recover your account">
      <RecoverForm />
    </AuthPageLayout>
  );
}
