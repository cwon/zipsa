/**
 * https://script.google.com/home/start 에서 새 프로젝트를 만든 뒤 붙여 넣고
 * createZipsaFeedbackForm을 실행하세요. 서버나 웹 앱으로 배포할 필요는 없습니다.
 * 실행 로그에 폼 편집 주소와 feedback.config.json에 넣을 설정이 표시됩니다.
 */
function createZipsaFeedbackForm() {
  const properties = PropertiesService.getScriptProperties();
  const existingId = properties.getProperty("ZIPSA_FEEDBACK_FORM_ID");
  let form;

  if (existingId) {
    // 같은 프로젝트에서 다시 실행해도 폼과 질문을 중복 생성하지 않습니다.
    form = FormApp.openById(existingId);
  } else {
    form = FormApp.create("집사 · Zipsa 의견 보내기", false);
    form
      .setDescription(
        "집사를 사용하며 불편했던 점이나 바라는 기능을 알려 주세요.\n\n" +
          "작성한 의견과 답변용 이메일(선택)은 Google Forms에 저장되며 집사 개발자가 확인합니다. " +
          "앱에서 이 폼을 열면 앱 버전과 운영체제만 미리 채웁니다. " +
          "공간·아이템·사진·할 일 데이터는 자동으로 보내지 않습니다. " +
          "의견 내용에도 주소, 구매 내역 등 개인 생활 정보는 넣지 말아 주세요.",
      )
      .setConfirmationMessage(
        "의견을 남겨 주셔서 감사합니다. 집사를 개선하는 데 참고하겠습니다.",
      )
      .setCollectEmail(false)
      .setLimitOneResponsePerUser(false)
      .setPublishingSummary(false)
      .setShowLinkToRespondAgain(false)
      .setAllowResponseEdits(false);

    form
      .addMultipleChoiceItem()
      .setTitle("어떤 의견인가요?")
      .setChoiceValues(["버그 제보", "기능 제안", "기타"])
      .setRequired(true);
    form
      .addParagraphTextItem()
      .setTitle("의견 내용")
      .setHelpText(
        "버그라면 어떤 순서로 사용했을 때 발생했는지, 기대했던 동작과 실제 동작을 알려 주세요.",
      )
      .setRequired(true);
    form
      .addTextItem()
      .setTitle("답변받을 이메일 (선택)")
      .setHelpText("답변을 받고 싶은 경우에만 적어 주세요.")
      .setValidation(
        FormApp.createTextValidation().requireTextIsEmail().build(),
      )
      .setRequired(false);
    form
      .addTextItem()
      .setTitle("앱 버전")
      .setHelpText("앱에서 열면 자동으로 입력됩니다.")
      .setRequired(false);
    form
      .addTextItem()
      .setTitle("운영체제")
      .setHelpText("앱에서 열면 macOS 또는 Windows가 자동으로 입력됩니다.")
      .setRequired(false);

    // 질문을 모두 만든 후 ID를 보관합니다. 이후 실행은 기존 폼을 사용합니다.
    properties.setProperty("ZIPSA_FEEDBACK_FORM_ID", form.getId());
  }

  const fields = form.getItems(FormApp.ItemType.TEXT);
  const version = fields.find((item) => item.getTitle() === "앱 버전");
  const os = fields.find((item) => item.getTitle() === "운영체제");
  if (!version || !os) {
    throw new Error("앱 버전과 운영체제 질문을 확인해 주세요.");
  }
  const prefilledUrl = form
    .createResponse()
    .withItemResponse(version.asTextItem().createResponse("__ZIPSA_VERSION__"))
    .withItemResponse(os.asTextItem().createResponse("__ZIPSA_OS__"))
    .toPrefilledUrl();

  form.setPublished(true);
  console.log("폼 편집: " + form.getEditUrl());
  console.log("응답용 주소: " + form.getPublishedUrl());
  console.log("앱 연결 설정 (feedback.config.json):");
  console.log(JSON.stringify({ prefilledUrl }, null, 2));
}
