import { Button } from "@/components/ui/button";
import { useLauncherStore } from "@/state";
import { AnimatePresence, m, type Variants } from "framer-motion";
import {
  CheckCircle2,
  ChevronRight,
  Gamepad2,
  UserCircle2,
} from "lucide-react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { playStartupSound } from "@/lib/audio";
import { ProfileSelector } from "./ProfileSelector";

interface OnboardingProps {
  onComplete: () => void;
}

const containerVariants: Variants = {
  hidden: { opacity: 0, scale: 0.95 },
  visible: {
    opacity: 1,
    scale: 1,
    transition: { duration: 0.5, type: "spring", bounce: 0.3 },
  },
  exit: { opacity: 0, scale: 1.05, transition: { duration: 0.3 } },
};

export const Onboarding = ({ onComplete }: OnboardingProps) => {
  const { t } = useTranslation();
  const [step, setStep] = useState(0);
  const { profiles } = useLauncherStore();

  useEffect(() => {
    playStartupSound();
  }, []);

  const handleNext = () => setStep((s) => s + 1);

  return (
    <div className="bg-background/65 fixed inset-0 z-50 flex items-center justify-center p-4 backdrop-blur-sm">
      <AnimatePresence mode="wait">
        {step === 0 && (
          <m.div
            key="step0"
            variants={containerVariants}
            initial="hidden"
            animate="visible"
            exit="exit"
            className="border-border/50 bg-card flex w-full max-w-md flex-col items-center justify-center gap-6 rounded-2xl border p-10 text-center shadow-2xl"
          >
            <div className="bg-primary/10 text-primary flex h-24 w-24 items-center justify-center rounded-full shadow-inner">
              <Gamepad2 className="h-12 w-12" />
            </div>
            <div>
              <h1 className="mb-2 text-3xl font-bold">
                {t("onboarding.welcome")}
              </h1>
              <p className="text-muted-foreground text-sm leading-relaxed">
                {t("onboarding.description")}
              </p>
            </div>

            <Button
              className="mt-2 h-10 w-full cursor-pointer items-center justify-center gap-2 rounded-xl text-sm font-semibold shadow-sm transition-transform active:scale-[0.98]"
              onClick={handleNext}
            >
              <span>{t("onboarding.getStarted")}</span>
              <ChevronRight className="h-4 w-4 shrink-0" />
            </Button>
          </m.div>
        )}

        {step === 1 && (
          <m.div
            key="step1"
            variants={containerVariants}
            initial="hidden"
            animate="visible"
            exit="exit"
            className="border-border/50 bg-card flex w-full max-w-md flex-col items-center justify-center gap-6 rounded-2xl border p-10 shadow-2xl"
          >
            <div className="bg-primary/10 text-primary flex h-16 w-16 items-center justify-center rounded-full shadow-inner">
              <UserCircle2 className="h-8 w-8" />
            </div>
            <div className="text-center">
              <h2 className="mb-2 text-2xl font-bold">
                {t("onboarding.profilesTitle")}
              </h2>
              <p className="text-muted-foreground text-sm">
                {t("onboarding.profilesDesc")}
              </p>
            </div>
            <div className="border-border/50 bg-background w-full rounded-xl border p-4 shadow-inner">
              <ProfileSelector />
            </div>
            <Button
              className="mt-4 h-10 w-full cursor-pointer items-center justify-center gap-2 rounded-xl text-sm font-semibold shadow-sm transition-transform active:scale-[0.98]"
              variant={profiles.length > 0 ? "default" : "secondary"}
              onClick={handleNext}
            >
              <span>{t("onboarding.continue")}</span>
              <ChevronRight className="h-4 w-4 shrink-0" />
            </Button>
          </m.div>
        )}

        {step === 2 && (
          <m.div
            key="step2"
            variants={containerVariants}
            initial="hidden"
            animate="visible"
            exit="exit"
            className="border-border/50 bg-card flex w-full max-w-md flex-col items-center justify-center gap-6 rounded-2xl border p-10 text-center shadow-2xl"
          >
            <div className="bg-primary/10 text-primary flex h-24 w-24 items-center justify-center rounded-full shadow-inner">
              <CheckCircle2 className="h-12 w-12" />
            </div>
            <div>
              <h2 className="mb-2 text-3xl font-bold">
                {t("onboarding.readyTitle")}
              </h2>
              <p className="text-muted-foreground">
                {t("onboarding.readyDesc")}
              </p>
            </div>
            <Button
              className="mt-4 h-10 w-full cursor-pointer items-center justify-center gap-2 rounded-xl text-sm font-semibold shadow-sm transition-transform active:scale-[0.98]"
              onClick={onComplete}
            >
              <span>{t("onboarding.finish")}</span>
            </Button>
          </m.div>
        )}
      </AnimatePresence>
    </div>
  );
};
