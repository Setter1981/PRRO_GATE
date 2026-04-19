using System;
using System.IO;
using System.Runtime.InteropServices;
using System.Threading;
using System.Windows.Forms;
using Microsoft.VisualBasic.CompilerServices;
using Microsoft.VisualBasic.FileIO;
using TaxGrpc;

namespace WebCheck;

[StructLayout(LayoutKind.Sequential, Size = 1)]
internal struct SubmitPtr
{
	internal TypErrSubmit SubmitCheck(string pathFile, string numbCheck, byte typCheck, long dd, string idCancel = "", string idOffline = "", bool OpenCloseShift = false)
	{
		All.tMaxS = "";
		TypErrSubmit typErrSubmit = default(TypErrSubmit);
		typErrSubmit.errCode = 0;
		typErrSubmit.errStr = "";
		typErrSubmit.returnNumber = "";
		typErrSubmit.returnStatus = -81;
		typErrSubmit.returnStr = "";
		typErrSubmit.returnData = "";
		string text = "";
		int num = 7;
		TypErrSubmit result;
		checked
		{
			int number;
			if (Versioned.IsNumeric((object)numbCheck))
			{
				number = Conversions.ToInteger(numbCheck);
			}
			else
			{
				string lastLocalCheckNumbern = All.l.ReturnLocalCheckNumberShift().LastLocalCheckNumbern;
				if (Versioned.IsNumeric((object)lastLocalCheckNumbern))
				{
					number = Conversions.ToInteger(lastLocalCheckNumbern);
					number = ((number > 0) ? (number + 1) : 0);
				}
				else
				{
					number = 0;
				}
			}
			try
			{
				int retries = All.Retries;
				for (int i = 1; i <= retries; i++)
				{
					Client client = new Client();
					client.Open(All.A.FiscalMode);
					Answer answer = client.Check(All.A.verAPI, pathFile, All.A.FN, dd, number, typCheck, idCancel, idOffline, All.Timing);
					Application.DoEvents();
					typErrSubmit.returnStatus = answer.Status;
					typErrSubmit.returnNumber = answer.Id.Replace("_", "`");
					typErrSubmit.returnStr = answer.IdSign;
					typErrSubmit.returnData = answer.IdData;
					text = answer.Message.Replace("_", " ");
					if (answer.Status == 1)
					{
						break;
					}
					if (answer.Status == -3)
					{
						if (num >= 1)
						{
							num--;
							Thread.Sleep(333);
							All.Lg.SaveTextToLog("SubmitCheck", "Сервер вернул ошибку -3", "Попытка повторной отправки");
							continue;
						}
						break;
					}
					if (unchecked(answer.Status == -15 && OpenCloseShift))
					{
						Thread.Sleep(333);
						TypErrMacNumDatDi typErrMacNumDatDi = new CheckLastCheck().LastCheckAllInfa();
						if (Conversions.ToInteger(All.l.MaxID("ksef").ReturnStr) + 1 == typErrMacNumDatDi.returnDI)
						{
							typErrSubmit.returnStatus = 1;
							typErrSubmit.returnNumber = typErrMacNumDatDi.returnNumber;
							typErrSubmit.returnStr = typErrMacNumDatDi.returnStr;
							typErrSubmit.returnData = typErrMacNumDatDi.returnData;
							break;
						}
						continue;
					}
					if (answer.Status == -16)
					{
						if (Conversions.ToInteger(All.l.MaxID("CHECKHEAD").ReturnStr) >= 1 || All.OfflineOnTechno().errCode > 0)
						{
							continue;
						}
						typErrSubmit.errCode = 97;
						typErrSubmit.errStr = "Увага! Аварійне онулення даних ПРРО. Виконано технічне включення оффлайн режиму.";
						result = typErrSubmit;
					}
					else
					{
						if (!unchecked(answer.Status == -2 && OpenCloseShift))
						{
							if (answer.Status == 0)
							{
								Thread.Sleep(333);
								TypErrMacNumDatDi typErrMacNumDatDi2 = new CheckLastCheck().LastCheckAllInfa();
								if (Conversions.ToInteger(All.l.MaxID("ksef").ReturnStr) + 1 == typErrMacNumDatDi2.returnDI)
								{
									typErrSubmit.returnStatus = 1;
									typErrSubmit.returnNumber = typErrMacNumDatDi2.returnNumber;
									typErrSubmit.returnStr = typErrMacNumDatDi2.returnStr;
									typErrSubmit.returnData = typErrMacNumDatDi2.returnData;
									break;
								}
							}
							else if (answer.Status < -1)
							{
								break;
							}
							continue;
						}
						if (!ErrorMessageOpenShift(answer.Message) || Conversions.ToInteger(All.l.MaxID("SHIFTS").ReturnStr) >= 1 || All.d.OpenShift().errCode > 0)
						{
							Thread.Sleep(333);
							TypErrMacNumDatDi typErrMacNumDatDi3 = new CheckLastCheck().LastCheckAllInfa();
							if (Conversions.ToInteger(All.l.MaxID("ksef").ReturnStr) + 1 == typErrMacNumDatDi3.returnDI)
							{
								typErrSubmit.returnStatus = 1;
								typErrSubmit.returnNumber = typErrMacNumDatDi3.returnNumber;
								typErrSubmit.returnStr = typErrMacNumDatDi3.returnStr;
								typErrSubmit.returnData = typErrMacNumDatDi3.returnData;
								break;
							}
							continue;
						}
						typErrSubmit.errCode = 95;
						typErrSubmit.errStr = "Увага! Аварійне онулення даних ПРРО.Виконано технічне відкриття зміни.Підсумки за зміну онулені.";
						result = typErrSubmit;
					}
					goto IL_05ca;
				}
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				typErrSubmit.errCode = 25;
				typErrSubmit.errStr = "Помилка надсилання чека для фіскалізації";
				result = typErrSubmit;
				ProjectData.ClearProjectError();
				goto IL_05ca;
			}
			if ((typErrSubmit.returnStatus == 0) | (typErrSubmit.returnStatus == -1))
			{
				All.Lg.SaveTextToLog("SubmitCheck", "ReturnError = " + typErrSubmit.returnStatus, "Message = " + text);
				_ = typErrSubmit.returnStatus;
				_ = -1;
				if (!All.l.OfflineTrue())
				{
					if (All.A.FullVersion)
					{
						if (All.OfflineAllowed)
						{
							TypErr typErr = All.OfflineOn();
							if (typErr.errCode > 0)
							{
								typErrSubmit.errCode = typErr.errCode;
								typErrSubmit.errStr = typErr.errStr;
								result = typErrSubmit;
							}
							else
							{
								typErrSubmit.errCode = 32;
								typErrSubmit.errStr = "Включен офлайн режим";
								result = typErrSubmit;
							}
						}
						else
						{
							typErrSubmit.errCode = 33;
							typErrSubmit.errStr = "Переход в офлайн режим запрещен";
							result = typErrSubmit;
						}
					}
					else
					{
						typErrSubmit.errCode = 34;
						typErrSubmit.errStr = "Нет интернета, переход в офлайн режим невозможен, бесплатная версия";
						result = typErrSubmit;
					}
					goto IL_05ca;
				}
			}
			else if (typErrSubmit.returnStatus < 1)
			{
				typErrSubmit.returnStr = ErrNumToString(typErrSubmit.returnStatus);
				typErrSubmit.errCode = 25;
				typErrSubmit.errStr = "Сервер податкової не прийняв чек";
				if (text.Trim().Length > 0)
				{
					typErrSubmit.errStr += ErrorMessageUA(text);
				}
			}
			else if (typErrSubmit.returnStatus != 1)
			{
				typErrSubmit.returnStr = ErrNumToString(typErrSubmit.returnStatus);
				typErrSubmit.errStr = "Помилка відправлення чека";
				typErrSubmit.errCode = 25;
			}
			else if (Operators.CompareString(typErrSubmit.returnNumber.Trim(), "", false) == 0)
			{
				typErrSubmit.errCode = 25;
				typErrSubmit.errStr = "Помилка відправлення чека";
			}
			DelTempFile(pathFile);
			result = typErrSubmit;
			goto IL_05ca;
		}
		IL_05ca:
		return result;
	}

	internal TypErrSubmit LastCheck(string pathFile)
	{
		All.tMaxS = "";
		TypErrSubmit typErrSubmit = default(TypErrSubmit);
		typErrSubmit.errCode = 0;
		typErrSubmit.errStr = "";
		typErrSubmit.returnNumber = "";
		typErrSubmit.returnStatus = -81;
		typErrSubmit.returnStr = "";
		typErrSubmit.returnData = "";
		TypErrSubmit result;
		try
		{
			int retries = All.Retries;
			for (int i = 1; i <= retries; i = checked(i + 1))
			{
				Client client = new Client();
				client.Open(All.A.FiscalMode);
				Answer answer = client.CheckLast(pathFile, All.Timing);
				Application.DoEvents();
				typErrSubmit.returnStatus = answer.Status;
				typErrSubmit.returnNumber = answer.Id.Replace("_", "`");
				typErrSubmit.returnStr = answer.IdSign;
				typErrSubmit.returnData = answer.IdData;
				if (answer.Status == 1 || (answer.Status < 0 && answer.Status < -1))
				{
					break;
				}
			}
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			typErrSubmit.errCode = 25;
			typErrSubmit.errStr = "Ошибка отправки запроса на последний чек";
			result = typErrSubmit;
			ProjectData.ClearProjectError();
			goto IL_027d;
		}
		if ((typErrSubmit.returnStatus == 0) | (typErrSubmit.returnStatus == -1))
		{
			All.Lg.SaveTextToLog("LastCheck", "ReturnError = " + typErrSubmit.returnStatus);
			if (!All.l.OfflineTrue())
			{
				if (All.A.FullVersion)
				{
					if (All.OfflineAllowed)
					{
						TypErr typErr = All.OfflineOn();
						if (typErr.errCode > 0)
						{
							typErrSubmit.errCode = typErr.errCode;
							typErrSubmit.errStr = typErr.errStr;
							result = typErrSubmit;
						}
						else
						{
							typErrSubmit.errCode = 32;
							typErrSubmit.errStr = "Включен офлайн режим";
							result = typErrSubmit;
						}
					}
					else
					{
						typErrSubmit.errCode = 33;
						typErrSubmit.errStr = "Переход в офлайн режим запрещен";
						result = typErrSubmit;
					}
				}
				else
				{
					typErrSubmit.errCode = 34;
					typErrSubmit.errStr = "Нет интернета, переход в офлайн режим невозможен, бесплатная версия";
					result = typErrSubmit;
				}
				goto IL_027d;
			}
		}
		else if (typErrSubmit.returnStatus != 1)
		{
			typErrSubmit.returnStr = ErrNumToString(typErrSubmit.returnStatus);
			typErrSubmit.errCode = 31;
			typErrSubmit.errStr = "Помилка отримання останнього чека з податкової";
		}
		else if (Operators.CompareString(typErrSubmit.returnData.Trim(), "", false) == 0)
		{
			typErrSubmit.errCode = 0;
			typErrSubmit.errStr = "";
			All.Lg.SaveTextToLog("LastCheck", "Отправлен запрос для получения последнего чека", "Ответ = ПустаяСтрока");
		}
		result = typErrSubmit;
		goto IL_027d;
		IL_027d:
		return result;
	}

	private string ErrorMessageUA(string emENG)
	{
		return " (" + emENG.Trim().Length switch
		{
			21 => "зміна вже відкрита", 
			54 => "цим підписом відкрита зміна на іншому ПРРО " + ErrorMessageUAfn(emENG), 
			46 => "у зміні може бути лише один підписант", 
			77 => "у зміні може бути лише один підписант, закриття зміни може бути здійснене старшим касиром", 
			38 => "можливо використовувати тільки з 01.10. 2021", 
			14 => "невірний хеш попереднього чеку, або дубль чека", 
			17 => "невірний xml Z-звіту", 
			_ => emENG, 
		} + ")";
	}

	private string ErrorMessageUAfn(string emENG)
	{
		string text = "";
		try
		{
			text += Conversions.ToString(emENG[44]);
			text += Conversions.ToString(emENG[45]);
			text += Conversions.ToString(emENG[46]);
			text += Conversions.ToString(emENG[47]);
			text += Conversions.ToString(emENG[48]);
			text += Conversions.ToString(emENG[49]);
			text += Conversions.ToString(emENG[50]);
			text += Conversions.ToString(emENG[51]);
			text += Conversions.ToString(emENG[52]);
			text += Conversions.ToString(emENG[53]);
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			text = "0000000000";
			ProjectData.ClearProjectError();
		}
		if (!Versioned.IsNumeric((object)text))
		{
			return "0000000000";
		}
		return text;
	}

	private bool ErrorMessageOpenShift(string emENG)
	{
		if (emENG.Trim().Length == 21)
		{
			return true;
		}
		return false;
	}

	private string ErrNumToString(int errNum)
	{
		return errNum switch
		{
			0 => "немає звязку з сервером ДПС", 
			-1 => "помилка перевірки підпису", 
			-2 => "помилка перевірки РРО", 
			-3 => "помилка запису", 
			-4 => "загальна помилка", 
			-5 => "помилка типу посилки", 
			-6 => "нема Z-звіту за попередній день", 
			-7 => "невірний формат XML (структура, фіскальний номер)", 
			-8 => "невірний формат XML дата не відповідає Check.date", 
			-9 => "невірний формат XML чеку", 
			-10 => "невірний формат Z-звіту", 
			-11 => "РРО заблокований, перевищено ліміт 168 годин офлайну", 
			-12 => "невірний хеш попереднього чеку", 
			-13 => "не зареєстровано ПРРО", 
			-14 => "не зареєстровано підписант", 
			-15 => "не відкрита зміна", 
			-16 => "невірний оффлайн ID", 
			_ => "не визначений", 
		};
	}

	internal void DelTempFile(string PathFile)
	{
		string text = PathFile;
		text = text.TrimEnd(new char[1] { 's' });
		text = text.TrimEnd(new char[1] { '7' });
		text = text.TrimEnd(new char[1] { 'p' });
		text = text.TrimEnd(new char[1] { '.' });
		All.tMaxS = SHA.GenerateSHA256File(text, Old: false);
		try
		{
			if (File.Exists(PathFile))
			{
				FileSystem.DeleteFile(PathFile);
			}
			if (File.Exists(text))
			{
				if (All.A.DelTempCheck > 2)
				{
					FileSystem.DeleteFile(text);
				}
				else if (All.A.DelTempCheck == 2 && !CheckZ(text))
				{
					FileSystem.DeleteFile(text);
				}
			}
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			ProjectData.ClearProjectError();
		}
	}

	private bool CheckZ(string PathFile)
	{
		bool result;
		try
		{
			result = ((Operators.CompareString(PathFile[checked(PathFile.Length - 5)].ToString().ToLower(), "z", false) == 0) ? true : false);
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result = false;
			ProjectData.ClearProjectError();
		}
		return result;
	}
}
