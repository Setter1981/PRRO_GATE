using System;
using System.Windows.Forms;
using System.Xml;
using Microsoft.VisualBasic.CompilerServices;

namespace WebCheck;

internal class Offlin
{
	public TypErr NewBackupNumbers(int nZ = 0)
	{
		TypErr typErr = default(TypErr);
		typErr.errCode = 0;
		typErr.errStr = "";
		TypErr result;
		checked
		{
			if (Operators.CompareString(All.A.FN, "7000000512", false) == 0)
			{
				typErr.errCode = 36;
				typErr.errStr = "В демонстрационной версии нет офлайн режим";
				result = typErr;
			}
			else if (!All.A.FullVersion)
			{
				typErr.errCode = 36;
				typErr.errStr = "Беслатная версия, офлайн режим не предусмотрен";
				All.Lg.SaveTextToLog("NewBackupNumbers", "", "Беслатная версия, офлайн режим не предусмотрен.");
				result = typErr;
			}
			else if (All.A.AutomatOfflineOn)
			{
				typErr.errCode = 55;
				typErr.errStr = "Работает автоматический режим офлайн";
				result = typErr;
			}
			else
			{
				NumbersOfflineUse numbersOfflineUse = new NumbersOfflineUse();
				if (!All.A.Status)
				{
					typErr.errCode = 36;
					typErr.errStr = "Нельзя получить номера не инициализировав рабочее место";
					All.Lg.SaveTextToLog("NewBackupNumbers", "", "Ошибка. Нельзя получить номера не инициализировав рабочее место.");
					result = typErr;
				}
				else if (All.l.OfflineTrue())
				{
					typErr.errCode = 36;
					typErr.errStr = "Нельзя получить номера будучи в офлайн режиме";
					All.Lg.SaveTextToLog("NewBackupNumbers", "", "Ошибка. Нельзя получить номера будучи в офлайн режиме.");
					result = typErr;
				}
				else
				{
					int num = Conversions.ToInteger(All.l.ReturnOpenShift().ReturnStr);
					if (num < 1)
					{
						typErr.errCode = 8;
						typErr.errStr = "Ошибка получения офлайн номеров";
						All.Lg.SaveTextToLog("NewBackupNumbers", "", "Ошибка при получении номера открытой смены.");
						result = typErr;
					}
					else
					{
						int num2 = numbersOfflineUse.CountNubmers();
						string text = All.f.StringGetFn(All.A.FN, "OfflineMax");
						int num3;
						if (Versioned.IsNumeric((object)text))
						{
							num3 = Conversions.ToInteger(text);
						}
						else
						{
							num3 = 500;
							All.f.StringWriteFN(All.A.FN, "OfflineMax", num3.ToString());
						}
						string text2 = All.f.StringGetFn(All.A.FN, "OfflineMin");
						int num4;
						if (Versioned.IsNumeric((object)text2))
						{
							num4 = Conversions.ToInteger(text2);
						}
						else
						{
							num4 = 50;
							All.f.StringWriteFN(All.A.FN, "OfflineMin", num4.ToString());
						}
						if (num3 > 2000)
						{
							num3 = 2000;
						}
						if (num4 < 0)
						{
							num4 = 0;
						}
						if (nZ < 1)
						{
							if (num2 > num4)
							{
								result = typErr;
								goto IL_06a7;
							}
							nZ = num3;
						}
						int num5 = Conversions.ToInteger(All.l.MaxID("ksef").ReturnStr) + 1;
						long dd = All.СurrentCompDate();
						string text3 = StringXML(nZ, num5.ToString(), dd.ToString());
						All.MacTempOld = "";
						TypErrStr typErrStr = All.SubstitutePreviousMAC(text3, num.ToString());
						if (typErrStr.errCode > 0)
						{
							typErr.errCode = typErrStr.errCode;
							typErr.errStr = typErrStr.errStr;
							All.Lg.SaveTextToLog("NewBackupNumbers", text3, "Ошибка при получении МАКа предыдущего чека: " + typErr.errStr);
							result = typErr;
						}
						else
						{
							text3 = typErrStr.ReturnStr;
							string text4 = All.MyDoc() + "\\WebCheck\\Temp\\" + All.A.FN + "\\NewOfflineN.xml";
							All.SaveToFileText(text4, text3);
							TypErrOperKeyPass typErrOperKeyPass = All.l.OperatorKeyPass(num.ToString());
							if (typErrOperKeyPass.errCode > 0)
							{
								typErr.errCode = typErrOperKeyPass.errCode;
								typErr.errStr = typErrOperKeyPass.errStr;
								All.Lg.SaveTextToLog("NewBackupNumbers", text3, "Ошибка получения информации об операторе: " + typErrOperKeyPass.errStr);
								result = typErr;
							}
							else
							{
								TypErr typErr2 = All.SF.SignatureFile(typErrOperKeyPass.KeyFile.Trim(), typErrOperKeyPass.Pass.Trim(), text4);
								if (typErr2.errCode > 0)
								{
									typErr.errCode = typErr2.errCode;
									typErr.errStr = typErr2.errStr;
									All.Lg.SaveTextToLog("NewBackupNumbers", text3, "Ошибка при подписи файла: " + typErr.errStr);
									result = typErr;
								}
								else
								{
									string pathFile = text4;
									text4 += ".p7s";
									SubmitPtr submitPtr = default(SubmitPtr);
									TypErrSubmit typErrSubmit = submitPtr.SubmitCheck(text4, num5.ToString(), 3, dd);
									if (typErrSubmit.errCode > 0)
									{
										typErr.errCode = typErrSubmit.errCode;
										typErr.errStr = typErrSubmit.errStr + "  Status: " + typErrSubmit.returnStatus + "  Msg: " + typErrSubmit.returnStr + "   ";
										All.Lg.SaveTextToLog("NewBackupNumbers", text3, "Ошибка отправки файла " + typErr.errStr);
										result = typErr;
									}
									else
									{
										TypErrStr typErrStr2 = All.SF.DecodCheck(typErrSubmit.returnData);
										Application.DoEvents();
										if (typErrStr2.errCode > 0)
										{
											typErr.errCode = typErrStr2.errCode;
											typErr.errStr = typErrStr2.errStr;
											result = typErr;
										}
										else
										{
											XmlDocument xmlDocument = new XmlDocument();
											XmlDocument xmlDocument2 = new XmlDocument();
											try
											{
												xmlDocument.LoadXml(typErrStr2.ReturnStr);
											}
											catch (Exception ex)
											{
												ProjectData.SetProjectError(ex);
												Exception ex2 = ex;
												typErr.errCode = 48;
												typErr.errStr = "Ошибка - формат XML ответа.";
												All.Lg.SaveTextToLog("NewBackupNumbers", text3, "Ошибка формата XML");
												result = typErr;
												ProjectData.ClearProjectError();
												goto IL_06a7;
											}
											Application.DoEvents();
											numbersOfflineUse.UPDATEfns();
											Application.DoEvents();
											XmlNodeList elementsByTagName = xmlDocument.GetElementsByTagName("ID");
											int num6 = elementsByTagName.Count - 1;
											for (int i = 0; i <= num6; i++)
											{
												xmlDocument2.LoadXml(elementsByTagName[i].OuterXml);
												string innerText = xmlDocument2.GetElementsByTagName("ID")[0].InnerText;
												numbersOfflineUse.Add(innerText, num5.ToString());
												Application.DoEvents();
											}
											TypErr typErr3 = All.l.SaveXMLcheck("Service", text3, text3, typErrSubmit.returnData, typErrSubmit.returnNumber, "12", "0.00", pathFile);
											if (typErr3.errCode > 0)
											{
												typErr.errCode = typErr3.errCode;
												typErr.errStr = typErr3.errStr;
												All.Lg.SaveTextToLog("NewBackupNumbers", text3, "Ошибка записи в таблицу ksef");
												result = typErr;
											}
											else
											{
												string returnStr = All.l.MaxID("ksef").ReturnStr;
												TypPrintChecks typPrintChecks = All.Rf.CheckXMLNumber(returnStr, SearchID: true);
												new PrintExportCheck().ExportToFile(typPrintChecks.ReturnStr, typPrintChecks.ReturnStrTaxN);
												result = typErr;
											}
										}
									}
								}
							}
						}
					}
				}
			}
			goto IL_06a7;
		}
		IL_06a7:
		return result;
	}

	private string StringXML(int nZ, string di, string CCD)
	{
		string text = "112";
		if (All.A.verAPI == 1)
		{
			text = "12";
		}
		string text2 = "<RQ V = '1'>";
		text2 = text2 + "<DAT FN='" + All.A.FN + "' TN='" + All.A.TIN + "' ZN='' DI='" + di + "' V='1'>";
		text2 = text2 + "<C T='" + text + "'><H SIZE='" + nZ + "'></H></C>";
		return text2 + "<TS>" + CCD + "</TS></DAT>mmmaaaccc</RQ>";
	}
}
