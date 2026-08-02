type CompletedProgress = {
  lecture_id: string;
  completed_at: Date | null;
};

type QuizAttempt = {
  passed: boolean;
  created_at: Date;
};

export function computeCourseCompletion(input: {
  lectureIds: string[];
  completedProgress: CompletedProgress[];
  lectureQuizAttempts: {
    lecture_id: string;
    latestAttempt: QuizAttempt | null;
  }[];
}): { is_completed: boolean; completed_at: Date | null } {
  const { lectureIds, completedProgress, lectureQuizAttempts } = input;

  if (lectureIds.length === 0) {
    return { is_completed: false, completed_at: null };
  }

  // O(L) — Set lookup instead of O(L²) nested .some() scan
  const completedLectureIds = new Set(
    completedProgress.map((p) => p.lecture_id),
  );

  const allLecturesCompleted = lectureIds.every((lectureId) =>
    completedLectureIds.has(lectureId),
  );

  const allQuizzesPassed = lectureQuizAttempts.every(
    ({ latestAttempt }) => latestAttempt?.passed === true,
  );

  const isCompleted = allLecturesCompleted && allQuizzesPassed;

  if (!isCompleted) {
    return { is_completed: false, completed_at: null };
  }

  const maxMs = completedProgress.reduce((max, p) => {
    if (p.completed_at instanceof Date) {
      return Math.max(max, p.completed_at.getTime());
    }
    return max;
  }, -Infinity);

  const finalMaxMs = lectureQuizAttempts.reduce((max, { latestAttempt }) => {
    if (latestAttempt?.created_at instanceof Date) {
      return Math.max(max, latestAttempt.created_at.getTime());
    }
    return max;
  }, maxMs);

  return {
    is_completed: true,
    completed_at: finalMaxMs === -Infinity ? null : new Date(finalMaxMs),
  };
}
