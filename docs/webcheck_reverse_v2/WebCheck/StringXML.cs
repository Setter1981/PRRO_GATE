using System;
using System.IO;
using System.Net.Http;
using System.Windows.Forms;
using System.Xml;
using Microsoft.VisualBasic;
using Microsoft.VisualBasic.CompilerServices;

namespace WebCheck;

internal class StringXML
{
	private const int razmer = 11;

	private XmlDocument x;

	private string[] t;

	private string[] Pay;

	private string uid;

	private string opertyp;

	private string idcancel;

	private string ver;

	private string CheckTaxNum;

	private string CheckIDv;

	public string xTAX;

	public string ZX;

	public string NumberShift;

	private string[] tegL;

	private TypDopTeg tegD;

	public StringXML()
	{
		x = new XmlDocument();
		t = new string[12];
		Pay = new string[181];
		uid = "";
		opertyp = "0";
		idcancel = "";
		ver = "1";
		CheckTaxNum = "";
		CheckIDv = "";
		xTAX = "";
		ZX = "";
		tegL = new string[101];
	}

	internal TypErr OpenShiftXML(string OperatorINN, string OperatorName)
	{
		TypErr result = default(TypErr);
		result.errCode = 0;
		result.errStr = "";
		string text = "108";
		if (All.A.verAPI == 1)
		{
			text = "8";
		}
		int num = checked(Conversions.ToInteger(All.l.MaxID("ksef").ReturnStr) + 1);
		long dd = All.СurrentCompDate();
		string text2 = "<RQ V='1'><DAT";
		string text3 = All.A.TIN;
		if (Versioned.IsNumeric(All.A.INN) && Convert.ToDouble(All.A.INN) > 0.0)
		{
			text3 = All.A.INN;
		}
		text2 = text2 + " FN='" + All.A.FN + "' TN='" + text3 + "' ZN='' DI='" + num + "' V='1'><C T='" + text + "'></C>";
		text2 += "<TS>";
		text2 += dd;
		text2 += "</TS></DAT>mmmaaaccc</RQ>";
		if (All.l.OfflineTrue())
		{
			All.OfflineNum = "";
			TypErrStr typErrStr = new NumbersOfflineUse().OfflineID();
			if (typErrStr.errCode > 0)
			{
				result.errCode = typErrStr.errCode;
				result.errStr = typErrStr.errStr;
				return result;
			}
			All.OfflineNum = typErrStr.ReturnStr;
			if (All.l.CloseOffline10())
			{
				result.errStr = "Ошибка записи оффлайн чека, сервер налоговой уже закрыл оффлайн режим. Повторите попытку.";
				result.errCode = 84;
				return result;
			}
			TypErr typErr = All.l.SaveOpenShift(OperatorName, All.A.TIN, All.A.PointName, OperatorINN);
			if (result.errCode > 0)
			{
				result.errStr = typErr.errStr;
				result.errCode = typErr.errCode;
				return result;
			}
			TypErr typErr2 = All.l.SaveXMLcheckOffline("Service", text2, text2, "not", typErrStr.ReturnStr, "8");
			if (typErr2.errCode > 0)
			{
				result.errCode = typErr2.errCode;
				result.errStr = typErr2.errStr;
				return result;
			}
			return result;
		}
		All.MacTempOld = "";
		TypErrStr typErrStr2 = All.SubstitutePreviousMAC(text2, OperatorINN, operINN: true);
		if (typErrStr2.errCode > 0)
		{
			result.errCode = typErrStr2.errCode;
			result.errStr = typErrStr2.errStr;
			return result;
		}
		text2 = typErrStr2.ReturnStr;
		text2 = Strings.Replace(text2, '\''.ToString(), '"'.ToString());
		string text4 = All.MyDoc() + "\\WebCheck\\Temp\\" + All.A.FN + "\\" + num + "a.xml";
		All.SaveToFileText(text4, text2);
		TypErrOperKeyPass typErrOperKeyPass = All.l.OperatorInfa(OperatorINN);
		if (typErrOperKeyPass.errCode > 0)
		{
			result.errCode = typErrOperKeyPass.errCode;
			result.errStr = typErrOperKeyPass.errStr;
			return result;
		}
		TypErr result2 = All.SF.SignatureFile(typErrOperKeyPass.KeyFile.Trim(), typErrOperKeyPass.Pass.Trim(), text4);
		if (result2.errCode > 0)
		{
			result.errCode = result2.errCode;
			result.errStr = result2.errStr;
			return result;
		}
		string pathFile = text4;
		text4 += ".p7s";
		CheckIDv = num.ToString();
		SubmitPtr submitPtr = default(SubmitPtr);
		TypErrSubmit typErrSubmit = submitPtr.SubmitCheck(text4, CheckIDv, 3, dd, "", "", OpenCloseShift: true);
		if (typErrSubmit.errCode > 0)
		{
			result2.errCode = typErrSubmit.errCode;
			result2.errStr = typErrSubmit.errStr + "  Status: " + typErrSubmit.returnStatus + "  Msg: " + typErrSubmit.returnStr + "   ";
			return result2;
		}
		if (typErrSubmit.returnStatus < 0)
		{
			result2.errCode = 26;
			result2.errStr = "Служебный чек не принят сервером. Ответ  Status: " + typErrSubmit.returnStatus + "  Msg:" + typErrSubmit.returnStr + "   ";
			return result2;
		}
		if (typErrSubmit.returnStatus == 0)
		{
			All.Lg.SaveTextToLog("OpenShiftXML", "Ошибка открытие смены", "Дополнительная сверка, с номером последнего чека, не дала результата");
			result2.errCode = 32;
			result2.errStr = "Переход в офлайн режим";
			return result2;
		}
		CheckTaxNum = typErrSubmit.returnNumber;
		TypErrStr typErrStr3 = All.l.ReturnOpenShift();
		if (typErrStr3.errCode > 0)
		{
			result.errCode = typErrStr3.errCode;
			result.errStr = typErrStr3.errStr;
			return result;
		}
		if (Conversions.ToInteger(typErrStr3.ReturnStr) > 1)
		{
			result.errCode = 0;
			result.errStr = "";
			return result;
		}
		result2 = All.l.SaveOpenShift(OperatorName, All.A.TIN, All.A.PointName, OperatorINN);
		if (result.errCode > 0)
		{
			result.errStr = result2.errStr;
			result.errCode = result2.errCode;
			return result;
		}
		result2 = All.l.SaveXMLcheck(num.ToString(), text2, text2, typErrSubmit.returnStr, typErrSubmit.returnNumber, "8", "0.00", pathFile);
		if (result2.errCode > 0)
		{
			result.errStr = result2.errStr;
			result.errCode = result2.errCode;
			return result;
		}
		return result;
	}

	internal TypErr OpenShift()
	{
		TypErr result = default(TypErr);
		result.errCode = 0;
		result.errStr = "";
		if (Conversions.ToInteger(All.l.ReturnOpenShift().ReturnStr) > 0)
		{
			result.errCode = 1006;
			result.errStr = "Вже є відкрита зміна";
			return result;
		}
		TypErrOperKeyPassNameINN typErrOperKeyPassNameINN = All.l.OperatorInfa();
		if (typErrOperKeyPassNameINN.errCode > 0)
		{
			result.errCode = typErrOperKeyPassNameINN.errCode;
			result.errStr = typErrOperKeyPassNameINN.errStr;
			return result;
		}
		result = All.l.SaveOpenShift(typErrOperKeyPassNameINN.Name, All.A.TIN, All.A.PointName, typErrOperKeyPassNameINN.INN);
		if (result.errCode > 0)
		{
			return result;
		}
		All.Lg.SaveTextToLog("OpenShift", "Увага! Після відновлення виконано технічне відкриття зміни.", "Інформація щодо поточної зміни може бути неповною.");
		return result;
	}

	public TypErrStr CheckProcessing(string xml, string NumShift)
	{
		CheckTaxNum = "";
		CheckIDv = "";
		All.NumberTaxVk = "";
		TypErrStr typErrStr = default(TypErrStr);
		typErrStr.errStr = "";
		typErrStr.errCode = 0;
		typErrStr.ReturnStr = "";
		NumberShift = NumShift;
		All.PayTax.ZeroTax();
		uid = "";
		idcancel = "";
		int num = 0;
		TypErrStr result;
		checked
		{
			do
			{
				Pay[num] = "";
				num++;
			}
			while (num <= 9);
			num = 0;
			do
			{
				t[num] = "";
				num++;
			}
			while (num <= 11);
			num = 0;
			do
			{
				tegL[num] = "";
				num++;
			}
			while (num <= 100);
			ref TypDopTeg reference = ref tegD;
			reference.PA = "";
			reference.PB = "";
			reference.PC = "";
			reference.PD = "";
			reference.PE = "";
			reference.PSNM = "";
			reference.RRN = "";
			reference.PF = "";
			reference.BID = "";
			reference.RID = "";
			reference.BTX = "";
			try
			{
				x.LoadXml(xml);
				TypErr typErr = Receipt();
				if (typErr.errCode > 0)
				{
					typErrStr.errCode = typErr.errCode;
					typErrStr.errStr = typErr.errStr;
					result = typErrStr;
					goto IL_01c2;
				}
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				typErrStr.errCode = 10;
				typErrStr.errStr = "Не правильный формат XML чека.";
				result = typErrStr;
				ProjectData.ClearProjectError();
				goto IL_01c2;
			}
			typErrStr.ReturnStr = "_CheckID=" + CheckTaxNum;
			All.NumberTaxVk = CheckTaxNum.Replace("`", "_");
			result = typErrStr;
			goto IL_01c2;
		}
		IL_01c2:
		return result;
	}

	private TypErr UidUn(string UidS)
	{
		TypErr result = default(TypErr);
		result.errCode = 0;
		result.errStr = "";
		if (!All.A.checkUID)
		{
			return result;
		}
		UidS = UidS.Trim();
		if (Operators.CompareString(UidS, "", TextCompare: false) == 0)
		{
			result.errCode = 88;
			result.errStr = "Ошибка. Необходимо обязательно указать тег UUID.";
			return result;
		}
		if (All.l.CountUID(UidS) > 0)
		{
			result.errCode = 88;
			result.errStr = "Ошибка. Такой UUID уже есть.";
			return result;
		}
		return result;
	}

	private TypErr Receipt()
	{
		TypErr typErr = default(TypErr);
		typErr.errCode = 0;
		typErr.errStr = "";
		try
		{
			uid = x.SelectSingleNode("/check/@uuid").Value;
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			uid = "";
			ProjectData.ClearProjectError();
		}
		typErr = UidUn(uid);
		TypErr result;
		checked
		{
			if (typErr.errCode > 0)
			{
				result = typErr;
			}
			else
			{
				try
				{
					opertyp = x.SelectSingleNode("/check/@operationtype").Value;
				}
				catch (Exception ex3)
				{
					ProjectData.SetProjectError(ex3);
					Exception ex4 = ex3;
					opertyp = "999";
					ProjectData.ClearProjectError();
				}
				if (!Versioned.IsNumeric(opertyp))
				{
					opertyp = "999";
				}
				if (Conversions.ToInteger(opertyp) != 0)
				{
					if (Conversions.ToInteger(opertyp) == 1)
					{
						try
						{
							idcancel = x.SelectSingleNode("/check/@idcancel").Value;
						}
						catch (Exception ex5)
						{
							ProjectData.SetProjectError(ex5);
							Exception ex6 = ex5;
							idcancel = "";
							ProjectData.ClearProjectError();
						}
					}
					else if (Math.Abs(Conversions.ToInteger(opertyp)) != 8)
					{
						typErr.errStr = "Неверно указан тип операции в чеке.";
						typErr.errCode = 19;
						result = typErr;
						goto IL_0738;
					}
				}
				ref TypDopTeg reference = ref tegD;
				try
				{
					reference.PA = x.SelectSingleNode("/check/l/@pa").Value;
				}
				catch (Exception ex7)
				{
					ProjectData.SetProjectError(ex7);
					Exception ex8 = ex7;
					reference.PA = "";
					ProjectData.ClearProjectError();
				}
				try
				{
					reference.PB = x.SelectSingleNode("/check/l/@pb").Value;
				}
				catch (Exception ex9)
				{
					ProjectData.SetProjectError(ex9);
					Exception ex10 = ex9;
					reference.PB = "";
					ProjectData.ClearProjectError();
				}
				try
				{
					reference.PC = x.SelectSingleNode("/check/l/@pc").Value;
				}
				catch (Exception ex11)
				{
					ProjectData.SetProjectError(ex11);
					Exception ex12 = ex11;
					reference.PC = "";
					ProjectData.ClearProjectError();
				}
				try
				{
					reference.PD = x.SelectSingleNode("/check/l/@pd").Value;
				}
				catch (Exception ex13)
				{
					ProjectData.SetProjectError(ex13);
					Exception ex14 = ex13;
					reference.PD = "";
					ProjectData.ClearProjectError();
				}
				try
				{
					reference.PE = x.SelectSingleNode("/check/l/@pe").Value;
				}
				catch (Exception ex15)
				{
					ProjectData.SetProjectError(ex15);
					Exception ex16 = ex15;
					reference.PE = "";
					ProjectData.ClearProjectError();
				}
				try
				{
					reference.PSNM = x.SelectSingleNode("/check/l/@psnm").Value;
				}
				catch (Exception ex17)
				{
					ProjectData.SetProjectError(ex17);
					Exception ex18 = ex17;
					reference.PSNM = "";
					ProjectData.ClearProjectError();
				}
				try
				{
					reference.RRN = x.SelectSingleNode("/check/l/@rrn").Value;
				}
				catch (Exception ex19)
				{
					ProjectData.SetProjectError(ex19);
					Exception ex20 = ex19;
					reference.RRN = "";
					ProjectData.ClearProjectError();
				}
				try
				{
					reference.PF = x.SelectSingleNode("/check/l/@pf").Value;
					if (reference.PF.Trim().Length > 0)
					{
						double num = All.StrToDouble(reference.PF);
						if (num == 0.0)
						{
							reference.PF = All.Bablo(0f);
						}
						else
						{
							reference.PF = All.Bablo(num);
						}
					}
				}
				catch (Exception ex21)
				{
					ProjectData.SetProjectError(ex21);
					Exception ex22 = ex21;
					reference.PF = "";
					ProjectData.ClearProjectError();
				}
				try
				{
					reference.BID = x.SelectSingleNode("/check/l/@bid").Value;
				}
				catch (Exception ex23)
				{
					ProjectData.SetProjectError(ex23);
					Exception ex24 = ex23;
					reference.BID = "";
					ProjectData.ClearProjectError();
				}
				try
				{
					reference.RID = x.SelectSingleNode("/check/l/@rid").Value;
				}
				catch (Exception ex25)
				{
					ProjectData.SetProjectError(ex25);
					Exception ex26 = ex25;
					reference.RID = "";
					ProjectData.ClearProjectError();
				}
				try
				{
					reference.BTX = x.SelectSingleNode("/check/l/@btx").Value;
				}
				catch (Exception ex27)
				{
					ProjectData.SetProjectError(ex27);
					Exception ex28 = ex27;
					reference.BTX = "";
					ProjectData.ClearProjectError();
				}
				try
				{
					tegL[0] = " email='" + x.SelectSingleNode("/check/l/@email").Value + "'";
				}
				catch (Exception ex29)
				{
					ProjectData.SetProjectError(ex29);
					Exception ex30 = ex29;
					tegL[0] = " email='0'";
					ProjectData.ClearProjectError();
				}
				if (All.A.RoundingCashINI)
				{
					All.A.RoundingCash = true;
				}
				else
				{
					string text;
					try
					{
						text = x.SelectSingleNode("/check/l/@roundingcash").Value;
					}
					catch (Exception ex31)
					{
						ProjectData.SetProjectError(ex31);
						Exception ex32 = ex31;
						text = "0";
						ProjectData.ClearProjectError();
					}
					if (Versioned.IsNumeric(text))
					{
						if (Conversions.ToInteger(text) > 0)
						{
							All.A.RoundingCash = true;
						}
						else
						{
							All.A.RoundingCash = false;
						}
					}
					else
					{
						All.A.RoundingCash = false;
					}
				}
				if (Operators.CompareString(idcancel.Trim(), "", TextCompare: false) != 0)
				{
					tegL[0] = tegL[0] + " idcancel='" + idcancel + "'";
				}
				bool flag = false;
				bool flag2 = false;
				int num2 = 1;
				do
				{
					if (!flag)
					{
						string xpath = "/check/l/@up" + num2;
						try
						{
							tegL[num2] = x.SelectSingleNode(xpath).Value;
							ref string reference2 = ref tegL[0];
							ref string reference3 = ref reference2;
							reference2 = reference3 + " up" + num2 + "='" + tegL[num2] + "'";
						}
						catch (Exception ex33)
						{
							ProjectData.SetProjectError(ex33);
							Exception ex34 = ex33;
							tegL[num2] = "";
							flag = true;
							ProjectData.ClearProjectError();
						}
					}
					if (!flag2)
					{
						string xpath2 = "/check/l/@dn" + num2;
						try
						{
							tegL[num2 + 50] = x.SelectSingleNode(xpath2).Value;
							ref string reference4 = ref tegL[0];
							ref string reference3 = ref reference4;
							reference4 = reference3 + " dn" + num2 + "='" + tegL[num2 + 50] + "'";
						}
						catch (Exception ex35)
						{
							ProjectData.SetProjectError(ex35);
							Exception ex36 = ex35;
							tegL[num2 + 50] = "";
							flag2 = true;
							ProjectData.ClearProjectError();
						}
					}
					if (unchecked(flag && flag2))
					{
						break;
					}
					num2++;
				}
				while (num2 <= 50);
				try
				{
					if (Operators.CompareString(x.SelectSingleNode("/check/@fn").Value.Trim().ToLower(), All.A.FN.Trim().ToLower(), TextCompare: false) != 0)
					{
						typErr.errStr = "Подключен друглй фискальный номер";
						typErr.errCode = 2;
						result = typErr;
						goto IL_0738;
					}
					typErr = Product();
					if (typErr.errCode > 0)
					{
						result = typErr;
						goto IL_0738;
					}
				}
				catch (Exception ex37)
				{
					ProjectData.SetProjectError(ex37);
					Exception ex38 = ex37;
					All.Lg.SaveTextToLog("Receipt", "Ошибка при обработке чека! Exception: " + ex38.Message);
					typErr.errStr = "Ошибка при обработке чека";
					typErr.errCode = 11;
					result = typErr;
					ProjectData.ClearProjectError();
					goto IL_0738;
				}
				result = typErr;
			}
			goto IL_0738;
		}
		IL_0738:
		return result;
	}

	private string DopTegE()
	{
		ref TypDopTeg reference = ref tegD;
		return string.Concat(string.Concat(string.Concat(string.Concat(string.Concat(string.Concat(string.Concat("" + "PA='" + reference.PA + "' ", "PB='", reference.PB, "' "), "PC='", reference.PC, "' "), "PD='", reference.PD, "' "), "PE='", reference.PE, "' "), "PSNM='", reference.PSNM, "' "), "RRN='", reference.RRN, "' "), "PF='", reference.PF, "' ");
	}

	private string DopTegM()
	{
		ref TypDopTeg reference = ref tegD;
		return string.Concat("" + "PSNM='" + reference.PSNM + "' ", "RRN='", reference.RRN, "' ");
	}

	private TypErr Product()
	{
		TypErr typErr = default(TypErr);
		typErr.errCode = 0;
		typErr.errStr = "";
		XmlNodeList elementsByTagName = x.GetElementsByTagName("good");
		checked
		{
			int num = elementsByTagName.Count - 1;
			string[,] array = new string[num + 1, 12];
			int num2 = num;
			int num3 = 0;
			TypErr result;
			double num6 = default(double);
			SubmitPtr submitPtr = default(SubmitPtr);
			while (true)
			{
				if (num3 <= num2)
				{
					TypErr typErr2 = Dereban(elementsByTagName[num3].OuterXml);
					if (typErr2.errCode == 0)
					{
						int num4 = 0;
						do
						{
							array[num3, num4] = t[num4];
							Application.DoEvents();
							num4++;
						}
						while (num4 <= 11);
						num3++;
						continue;
					}
					typErr.errStr = typErr2.errStr;
					typErr.errCode = typErr2.errCode;
					result = typErr;
					break;
				}
				int num5 = num;
				for (num3 = 0; num3 <= num5; num3++)
				{
					num6 += All.StrToDouble(array[num3, 2]);
					num6 = All.StrToDouble(All.Bablo(num6));
					Application.DoEvents();
				}
				XmlNodeList elementsByTagName2 = x.GetElementsByTagName("payment");
				int num7 = elementsByTagName2.Count - 1;
				num3 = 0;
				while (true)
				{
					if (num3 <= num7)
					{
						TypErrStr parametrToString = GetParametrToString(elementsByTagName2[num3].OuterXml, "id", "payment");
						if (parametrToString.errCode > 0)
						{
							typErr.errCode = parametrToString.errCode;
							typErr.errStr = parametrToString.errStr;
							result = typErr;
							break;
						}
						if (!Versioned.IsNumeric(parametrToString.ReturnStr))
						{
							typErr.errCode = 62;
							typErr.errStr = "Ошибка. Один из платежей указан неверно";
							result = typErr;
							break;
						}
						Conversions.ToInteger(parametrToString.ReturnStr);
						num3++;
						continue;
					}
					XmlNodeList elementsByTagName3 = x.GetElementsByTagName("payment");
					int num8 = 0;
					double num9 = 0.0;
					bool flag = false;
					double num10 = 0.0;
					int payN = All.PayTax.PayN;
					num3 = 1;
					while (true)
					{
						if (num3 <= payN)
						{
							try
							{
								string xpath = "/check/payments/payment[@id='" + num3 + "']/@sum";
								if (num3 == 1)
								{
									string xpath2 = "/check/payments/payment[@id='" + num3 + "']/@smb";
									try
									{
										num10 = All.StrToDouble(x.SelectSingleNode(xpath2).Value);
									}
									catch (Exception ex)
									{
										ProjectData.SetProjectError(ex);
										Exception ex2 = ex;
										num10 = 0.0;
										ProjectData.ClearProjectError();
									}
								}
								Pay[num3] = x.SelectSingleNode(xpath).Value;
								num8++;
								if (num3 > 1)
								{
									flag = true;
								}
								if (Operators.CompareString(All.PayTax.get_PayName(num3).Trim(), "", TextCompare: false) == 0)
								{
									typErr.errCode = 62;
									typErr.errStr = "Ошибка. Нет платежа с индексом " + num3;
									result = typErr;
									break;
								}
							}
							catch (Exception ex3)
							{
								ProjectData.SetProjectError(ex3);
								Exception ex4 = ex3;
								Pay[num3] = "0";
								ProjectData.ClearProjectError();
							}
							if (All.StrToDouble(Pay[num3]) < 0.0)
							{
								typErr.errCode = 1017;
								typErr.errStr = "Ошибка. Число не может быть отрицательным.";
								result = typErr;
								break;
							}
							All.PayTax.set_SumPay(num3, Pay[num3]);
							num9 += All.StrToDouble(Pay[num3]);
							num3++;
							continue;
						}
						if (num8 < elementsByTagName3.Count)
						{
							typErr.errCode = 72;
							typErr.errStr = "Помилка. ID платежів повторюються.";
							result = typErr;
							break;
						}
						if (All.A.DopNal > 0 && All.StrToDouble(Pay[1]) > (double)All.A.DopNal)
						{
							typErr.errCode = 70;
							typErr.errStr = "Сума готівкового платежу перевищує допустимий ліміт.";
							result = typErr;
							break;
						}
						double num11 = 0.0;
						double num12 = 0.0;
						double m = All.StrToDouble(All.Bablo(num6.ToString()));
						double num13 = All.StrToDouble(All.Bablo(num9.ToString()));
						double sBablo = All.StrToDouble(All.Bablo(num6.ToString()));
						double num14 = 0.0;
						double num15 = 0.0;
						string text = "";
						string smbS = "0";
						if (num10 > 0.0)
						{
							if (flag)
							{
								typErr.errCode = 102;
								typErr.errStr = "Не можна застосовувати округлення, якщо є безготівковий розрахунок.";
								result = typErr;
								break;
							}
							if (num10 != Okruglit(m, num10))
							{
								typErr.errCode = 102;
								typErr.errStr = "Помилка округлення суми чека.  SMB:" + num10 + "  Потрібно:" + Okruglit(m, num10);
								result = typErr;
								break;
							}
							text = "' SMB='" + All.Bablo(sBablo);
							if (num10 > num6)
							{
								num14 = num10 - num6;
								smbS = num14.ToString();
								num15 = 0.0;
								text = text + "' SMP='" + All.Bablo(num14);
							}
							else
							{
								num15 = num6 - num10;
								smbS = "-" + num15;
								num14 = 0.0;
								text = text + "' SMM='" + All.Bablo(num15);
							}
							num6 = num10;
						}
						m = num6;
						if (m > num13)
						{
							typErr.errCode = 13;
							typErr.errStr = "Сума платежів менша, ніж загальна сума чека.";
							result = typErr;
							break;
						}
						if (m < num13)
						{
							num11 = num9 - num6;
							num12 = All.StrToDouble(Pay[1]) - num11;
							if (num11 > All.StrToDouble(Pay[1]))
							{
								typErr.errCode = 64;
								typErr.errStr = "Сума здачі більша, ніж сума готівкової оплати в чеку.";
								result = typErr;
								break;
							}
							num9 = num6;
						}
						if (!All.A.FullVersion && m > 800.0)
						{
							typErr.errCode = 45;
							typErr.errStr = "У безкоштовній версії загальна сума чека не може перевищувати 800 гривень.";
							result = typErr;
							break;
						}
						if (Conversions.ToInteger(opertyp) > 0 && All.StrToDouble(Pay[1]) > All.Nal())
						{
							typErr.errCode = 47;
							typErr.errStr = "Помилка! У касі немає необхідної суми.";
							result = typErr;
							break;
						}
						long dd = All.СurrentCompDate();
						TypErrStr typErrStr = All.l.MaxID("ksef");
						if (typErrStr.errCode > 0)
						{
							typErr.errCode = typErrStr.errCode;
							typErr.errStr = typErrStr.errStr;
							result = typErr;
							break;
						}
						CheckIDv = (1 + Conversions.ToInteger(typErrStr.ReturnStr)).ToString();
						typErrStr.ReturnStr = CheckIDv;
						xTAX = "";
						TypErrLLCNshift typErrLLCNshift = All.l.ReturnLocalCheckNumberShift();
						if (typErrLLCNshift.errCode > 0)
						{
							typErr.errCode = typErrLLCNshift.errCode;
							typErr.errStr = typErrLLCNshift.errStr;
							result = typErr;
							break;
						}
						int num16 = 0;
						int num17 = Conversions.ToInteger(opertyp);
						if (num17 < 0)
						{
							num17 = Math.Abs(num17);
						}
						string text2 = All.A.TIN;
						if (Versioned.IsNumeric(All.A.INN) && Convert.ToDouble(All.A.INN) > 0.0)
						{
							text2 = All.A.INN;
						}
						ref string reference = ref xTAX;
						ref string reference2 = ref reference;
						reference = reference2 + "<DAT FN='" + All.A.FN + "' TN='" + text2 + "' DI='" + CheckIDv + "' ZN='0' V='1'>";
						ref string reference3 = ref xTAX;
						reference3 = reference3 + "<C T='" + num17 + "'>";
						int num18 = 1;
						while (tegL[num18].Trim().Length > 0)
						{
							num16++;
							ref string reference4 = ref xTAX;
							reference2 = ref reference4;
							reference4 = reference2 + "<L N='" + num16 + "'>" + tegL[num18] + "</L>";
							num18++;
							if (num18 > 50)
							{
								break;
							}
						}
						int num19 = num;
						for (num3 = 0; num3 <= num19; num3++)
						{
							AAAAA(array[num3, 8]);
							num16++;
							if (Conversions.ToInteger(opertyp) == -8)
							{
								ref string reference5 = ref xTAX;
								reference2 = ref reference5;
								reference5 = reference2 + "<P N='" + num16 + "' C='" + array[num3, 3] + "' NM='" + array[num3, 4] + "' SM='" + All.Bablo(array[num3, 2]) + "' Q='" + All.KolvoVes(array[num3, 0]) + "' PRC='" + All.Bablo(array[num3, 1]) + "' CD='" + array[num3, 9] + "' CZD='" + array[num3, 7] + AAAAA(array[num3, 8]);
							}
							else if (All.StrToDouble(array[num3, 10]) > 0.0)
							{
								string text3 = "' avans='" + All.Bablo(array[num3, 10]) + "' avansm='" + array[num3, 11];
								ref string reference6 = ref xTAX;
								reference2 = ref reference6;
								reference6 = reference2 + "<P N='" + num16 + "' C='" + array[num3, 3] + "' NM='" + array[num3, 4] + "' SM='" + All.Bablo(array[num3, 2]) + "' Q='" + All.KolvoVes(array[num3, 0]) + "' PRC='" + All.Bablo(array[num3, 1]) + "' TX='" + All.PayTax.ABCtoNUM(array[num3, 5]) + "' CD='" + array[num3, 9] + text3 + "' CZD='" + array[num3, 7] + AAAAA(array[num3, 8]);
							}
							else
							{
								ref string reference7 = ref xTAX;
								reference2 = ref reference7;
								reference7 = reference2 + "<P N='" + num16 + "' C='" + array[num3, 3] + "' NM='" + array[num3, 4] + "' SM='" + All.Bablo(array[num3, 2]) + "' Q='" + All.KolvoVes(array[num3, 0]) + "' PRC='" + All.Bablo(array[num3, 1]) + "' TX='" + All.PayTax.ABCtoNUM(array[num3, 5]) + "' CD='" + array[num3, 9] + "' CZD='" + array[num3, 7] + AAAAA(array[num3, 8]);
							}
							double num20 = Conversions.ToDouble(Discount(array[num3, 0], array[num3, 1], array[num3, 2]).ToString());
							if (num20 > 0.0)
							{
								num16++;
								ref string reference8 = ref xTAX;
								reference2 = ref reference8;
								reference8 = reference2 + "<D N='" + num16 + "' NI='" + (num16 - 1) + "' TX='" + All.PayTax.ABCtoNUM(array[num3, 5]) + "' SM='" + All.Bablo(num20.ToString()) + "' TR='0' TY='0'/>";
							}
							else if (num20 < 0.0)
							{
								num16++;
								ref string reference9 = ref xTAX;
								reference2 = ref reference9;
								reference9 = reference2 + "<S N='" + num16 + "' NI='" + (num16 - 1) + "' TX='" + All.PayTax.ABCtoNUM(array[num3, 5]) + "' SM='" + All.Bablo(Math.Abs(num20).ToString()) + "' TR='0' TY='0'/>";
							}
						}
						int payN2 = All.PayTax.PayN;
						for (num3 = 1; num3 <= payN2; num3++)
						{
							Directorys payTax = All.PayTax;
							if (All.StrToDouble(payTax.get_SumPay(num3)) > 0.0)
							{
								num16++;
								if (num3 != 1)
								{
									if (Conversions.ToInteger(opertyp) == -8)
									{
										ref string reference10 = ref xTAX;
										reference2 = ref reference10;
										reference10 = reference2 + "<M N='" + num16 + "' T='0' NM='" + All.NameNoDot(payTax.get_PayName(num3)) + "' SM='" + All.Bablo(payTax.get_SumPay(num3)) + "' " + DopTegE() + "/> ";
									}
									else
									{
										ref string reference11 = ref xTAX;
										reference2 = ref reference11;
										reference11 = reference2 + "<M N='" + num16 + "' T='" + payTax.get_PayISCASH(num3) + "' NM='" + All.NameNoDot(payTax.get_PayName(num3)) + "' SM='" + All.Bablo(payTax.get_SumPay(num3)) + "' " + DopTegE() + "/> ";
									}
								}
								else if (Conversions.ToInteger(opertyp) == 0)
								{
									ref string reference12 = ref xTAX;
									reference2 = ref reference12;
									reference12 = reference2 + "<M N='" + num16 + "' T='" + (num3 - 1) + "' NM='" + All.NameNoDot(payTax.get_PayName(num3)) + "' SM='" + All.Bablo(payTax.get_SumPay(num3)) + "' RM='" + All.Bablo(num11.ToString()) + text + "'/>";
								}
								else
								{
									ref string reference13 = ref xTAX;
									reference2 = ref reference13;
									reference13 = reference2 + " <M N='" + num16 + "' T='" + (num3 - 1) + "' NM='" + All.NameNoDot(payTax.get_PayName(num3)) + "' SM='" + All.Bablo(payTax.get_SumPay(num3)) + text + "'/>";
								}
							}
							payTax = null;
						}
						int num21 = Conversions.ToInteger(typErrLLCNshift.LastLocalCheckNumbern) + 1;
						num16++;
						if (num10 > 0.0)
						{
							ref string reference14 = ref xTAX;
							reference2 = ref reference14;
							reference14 = reference2 + "<E N='" + num16 + "' NO='" + num21 + "' SM='" + All.Bablo(sBablo) + "' TS='" + All.СurrentCompDate() + "' CS='" + typErrLLCNshift.OperatorID + "' " + DopTegE() + ">";
						}
						else
						{
							ref string reference15 = ref xTAX;
							reference2 = ref reference15;
							reference15 = reference2 + "<E N='" + num16 + "' NO='" + num21 + "' SM='" + All.Bablo(num9.ToString()) + "' TS='" + All.СurrentCompDate() + "' CS='" + typErrLLCNshift.OperatorID + "' " + DopTegE() + ">";
						}
						bool flag2 = false;
						if (All.PayTax.get_SumTax(4) > 0.0)
						{
							flag2 = true;
						}
						if (All.PayTax.get_SumTax(5) > 0.0)
						{
							flag2 = true;
						}
						bool flag3 = false;
						if (All.PayTax.get_SumTax(6) > 0.0)
						{
							flag3 = true;
						}
						if (All.PayTax.get_SumTax(7) > 0.0)
						{
							flag3 = true;
						}
						string text4;
						unchecked
						{
							if (flag2 && flag3)
							{
								typErr.errStr = "Ошибка! В чеке может быть только одна программируемая ставка.";
								typErr.errCode = 43;
								result = typErr;
								break;
							}
							text4 = "0.00";
							Directorys payTax2 = All.PayTax;
							int taxN = payTax2.TaxN;
							for (num3 = 1; num3 <= taxN; num3 = checked(num3 + 1))
							{
								if (!(payTax2.get_Sum(num3) > 0.0))
								{
									continue;
								}
								double num22 = payTax2.get_SumTax(num3);
								switch (num3)
								{
								case 1:
								{
									string text5 = "0";
									string sBablo2 = "0";
									string sBablo3 = "0.00";
									text4 = All.Bablo(num22.ToString());
									if (All.PayTax.get_SumTax(4) > 0.0)
									{
										num22 -= All.PayTax.get_TXSM(4);
									}
									else if (All.PayTax.get_SumTax(6) > 0.0)
									{
										num22 -= All.PayTax.get_TXSM(6);
									}
									if (num22 > 0.0)
									{
										ref string reference16 = ref xTAX;
										reference2 = ref reference16;
										reference16 = reference2 + "<TX TX='" + payTax2.get_TaxABCtoNum(num3) + "' TXPR='" + TaxDot(All.Bablo(payTax2.get_TaxPRC(num3)), dot: false) + "' TXSM='" + All.Bablo(num22.ToString()) + "' DTPR='" + TaxDot(All.Bablo(sBablo3), dot: false) + "' DTSM='" + All.Bablo(sBablo2) + "' TXTY='0' TXAL='" + text5 + "'/>";
									}
									continue;
								}
								case 2:
								{
									string text6 = "0";
									string sBablo4 = "0";
									string sBablo5 = "0.00";
									if (All.PayTax.get_SumTax(5) > 0.0)
									{
										num22 -= All.PayTax.get_TXSM(5);
									}
									else if (All.PayTax.get_SumTax(7) > 0.0)
									{
										num22 -= All.PayTax.get_TXSM(7);
									}
									ref string reference17 = ref xTAX;
									reference2 = ref reference17;
									reference17 = reference2 + "<TX TX='" + payTax2.get_TaxABCtoNum(num3) + "' TXPR='" + TaxDot(All.Bablo(payTax2.get_TaxPRC(num3)), dot: false) + "' TXSM='" + All.Bablo(num22.ToString()) + "' DTPR='" + TaxDot(All.Bablo(sBablo5), dot: false) + "' DTSM='" + All.Bablo(sBablo4) + "' TXTY='0' TXAL='" + text6 + "'/>";
									continue;
								}
								}
								if (num3 > 3 && num3 < 8)
								{
									string text7 = "2";
									string sBablo6 = num22.ToString();
									string sBablo7 = payTax2.get_TaxEXCISE(num3);
									ref string reference18 = ref xTAX;
									reference2 = ref reference18;
									reference18 = reference2 + "<TX TX='" + payTax2.get_TaxABCtoNum(num3) + "' TXPR='" + TaxDot(All.Bablo(payTax2.get_TaxPRC(num3)), dot: false) + "' TXSM='" + All.Bablo(payTax2.get_TXSM(num3).ToString()) + "' DTPR='" + TaxDot(All.Bablo(sBablo7), dot: false) + "' DTSM='" + All.Bablo(sBablo6) + "' TXTY='0' TXAL='" + text7 + "'/>";
								}
								else if (num3 == 3 || num3 > 7)
								{
									ref string reference19 = ref xTAX;
									reference2 = ref reference19;
									reference19 = reference2 + "<TX TX='" + payTax2.get_TaxABCtoNum(num3) + "' TXPR='" + TaxDot(All.Bablo(payTax2.get_TaxPRC(num3)), dot: false) + "' TXSM='" + All.Bablo(num22.ToString()) + "' DTPR='" + TaxDot(All.Bablo("0"), dot: false) + "' DTSM='0' TXTY='0' TXAL='0'/>";
								}
							}
							payTax2 = null;
							xTAX += "</E>";
							num18 = 51;
						}
						while (tegL[num18].Trim().Length > 0)
						{
							num16++;
							ref string reference20 = ref xTAX;
							reference2 = ref reference20;
							reference20 = reference2 + "<L N='" + num16 + "'>" + tegL[num18] + "</L>";
							num18++;
							if (num18 > 100)
							{
								break;
							}
						}
						if (Operators.CompareString(All.f.StringGetFn(All.A.FN, "useecheckmegovua"), "1", TextCompare: false) == 0 && tegD.RRN.Length > 0)
						{
							if (Operators.CompareString(tegD.BID, "", TextCompare: false) == 0)
							{
								tegD.BID = tegD.RRN;
							}
							if (Operators.CompareString(tegD.RID, "", TextCompare: false) == 0)
							{
								int num23 = Conversions.ToInteger(All.l.MaxID("ksef").ReturnStr) + 1;
								tegD.RID = num23.ToString();
							}
							num16++;
							ref string reference21 = ref xTAX;
							reference21 = reference21 + "<L N='" + num16 + "'>ERECEIPT</L>";
							num16++;
							ref string reference22 = ref xTAX;
							reference2 = ref reference22;
							reference22 = reference2 + "<L N='" + num16 + "'>BID=" + tegD.BID + "</L>";
							num16++;
							ref string reference23 = ref xTAX;
							reference2 = ref reference23;
							reference23 = reference2 + "<L N='" + num16 + "'>RID=" + tegD.RID + "</L>";
							num16++;
							ref string reference24 = ref xTAX;
							reference2 = ref reference24;
							reference24 = reference2 + "<L N='" + num16 + "'>BTX=" + tegD.BTX + "</L>";
							num16++;
							ref string reference25 = ref xTAX;
							reference2 = ref reference25;
							reference25 = reference2 + "<L N='" + num16 + "'>TIN=" + All.A.TIN + "</L>";
						}
						num16++;
						ref string reference26 = ref xTAX;
						reference2 = ref reference26;
						reference26 = reference2 + "<WebCheck N='" + num16 + "' TaxA='" + text4 + "'" + tegL[0] + "/>";
						xTAX += "</C>";
						ref string reference27 = ref xTAX;
						reference27 = reference27 + "<TS>" + dd + "</TS>";
						xTAX += "</DAT>";
						if (All.l.OfflineTrue())
						{
							xTAX = "<RQ V='1'>" + xTAX + "mmmaaaccc</RQ>";
							All.OfflineNum = "";
							TypErrStr typErrStr2 = new NumbersOfflineUse().OfflineID();
							if (typErrStr2.errCode > 0)
							{
								typErr.errCode = typErrStr2.errCode;
								typErr.errStr = typErrStr2.errStr;
								result = typErr;
								break;
							}
							All.OfflineNum = typErrStr2.ReturnStr;
							CheckTaxNum = typErrStr2.ReturnStr;
							if (All.l.CloseOffline10())
							{
								typErr.errStr = "Ошибка записи оффлайн чека, сервер налоговой уже закрыл оффлайн режим. Повторите попытку.";
								typErr.errCode = 84;
								result = typErr;
								break;
							}
							if (All.l.BagCloseOfflineShift())
							{
								typErr.errStr = "Помилка закриття зміни в оффлайн режимі , зробить пошук помилок для виправлення.";
								typErr.errCode = 104;
								result = typErr;
								break;
							}
							try
							{
								string expression = Strings.Replace(xTAX, ".", "");
								expression = Strings.Replace(expression, "#`#", ".");
								xTAX = Strings.Replace(xTAX, "#`#", ".");
								expression = Strings.Replace(expression, "~`~", " # ");
								int num24 = Conversions.ToInteger(All.l.MaxID("CHECKHEAD").ReturnStr) + 1;
								typErr = All.l.SaveXMLcheckOffline(num24.ToString(), TaxDot(xTAX), TaxDot(expression), "not", typErrStr2.ReturnStr, opertyp, All.Bablo(num6.ToString()));
								if (typErr.errCode > 0)
								{
									result = typErr;
									break;
								}
								typErr = All.l.SaveCheck(uid, num6.ToString(), opertyp, typErrStr2.ReturnStr, smbS);
								if (typErr.errCode > 0)
								{
									result = typErr;
									break;
								}
								XmlNodeList elementsByTagName4 = x.GetElementsByTagName("payment");
								int num25 = elementsByTagName4.Count - 1;
								num3 = 0;
								while (true)
								{
									if (num3 <= num25)
									{
										TypErrStr parametrToString2 = GetParametrToString(elementsByTagName4[num3].OuterXml, "id", "payment");
										if (parametrToString2.errCode > 0)
										{
											typErr.errCode = parametrToString2.errCode;
											typErr.errStr = parametrToString2.errStr;
											result = typErr;
											goto end_IL_034f;
										}
										TypErrStr parametrToString3 = GetParametrToString(elementsByTagName4[num3].OuterXml, "sum", "payment");
										if (parametrToString3.errCode > 0)
										{
											typErr.errCode = parametrToString3.errCode;
											typErr.errStr = parametrToString3.errStr;
											result = typErr;
											goto end_IL_034f;
										}
										typErr = ((Operators.CompareString(parametrToString2.ReturnStr, "1", TextCompare: false) != 0) ? All.l.SaveCheckPay(num24.ToString(), All.PayTax.get_PayName(Conversions.ToInteger(parametrToString2.ReturnStr)), parametrToString3.ReturnStr) : ((!(num11 > 0.0)) ? All.l.SaveCheckPay(num24.ToString(), All.PayTax.get_PayName(Conversions.ToInteger(parametrToString2.ReturnStr)), parametrToString3.ReturnStr) : All.l.SaveCheckPay(num24.ToString(), All.PayTax.get_PayName(Conversions.ToInteger(parametrToString2.ReturnStr)), num12.ToString())));
										if (typErr.errCode > 0)
										{
											result = typErr;
											goto end_IL_034f;
										}
										num3++;
										continue;
									}
									Directorys payTax3 = All.PayTax;
									int taxN2 = payTax3.TaxN;
									num3 = 1;
									while (true)
									{
										if (num3 <= taxN2)
										{
											if (!(payTax3.get_Sum(num3) > 0.0))
											{
												goto IL_1d85;
											}
											float num26 = (float)payTax3.get_SumTax(num3);
											typErr = All.l.SaveTaxa(num24.ToString(), payTax3.get_TaxName(num3), payTax3.get_TaxPRC(num3), All.Bablo(num26.ToString()));
											if (typErr.errCode <= 0)
											{
												goto IL_1d85;
											}
											result = typErr;
											goto end_IL_034f;
										}
										payTax3 = null;
										int num27 = num;
										for (num3 = 0; num3 <= num27; num3++)
										{
											typErr = All.l.SaveGood(num24.ToString(), array[num3, 3], array[num3, 7], array[num3, 4], array[num3, 0], array[num3, 1], array[num3, 5], array[num3, 2]);
											if (typErr.errCode > 0)
											{
												result = typErr;
												goto end_IL_034f;
											}
										}
										break;
										IL_1d85:
										num3++;
									}
									break;
								}
								goto IL_1e48;
							}
							catch (Exception ex5)
							{
								ProjectData.SetProjectError(ex5);
								Exception ex6 = ex5;
								typErr.errStr = "Ошибка записи товаров в таблицу при обработке офлайн чека: " + ex6.Message;
								typErr.errCode = 14;
								result = typErr;
								ProjectData.ClearProjectError();
							}
							break;
						}
						All.MacTempOld = "";
						TypErrStr typErrStr3 = NameDotNoDot(xTAX, sDot: true);
						if (typErrStr3.errCode > 0)
						{
							typErr.errCode = typErrStr3.errCode;
							typErr.errStr = typErrStr3.errStr;
							result = typErr;
							break;
						}
						xTAX = NameDotNoDot(xTAX, sDot: false).ReturnStr;
						xTAX = Strings.Replace(xTAX, "~`~", " # ");
						string text8 = All.MyDoc() + "\\WebCheck\\Temp\\" + All.A.FN + "\\" + CheckIDv + "G.xml";
						xTAX = TaxDot(xTAX);
						All.SaveToFileText(text8, xTAX);
						TypErrOperKeyPass typErrOperKeyPass = All.l.OperatorKeyPass(NumberShift);
						if (typErrOperKeyPass.errCode > 0)
						{
							typErr.errCode = typErrOperKeyPass.errCode;
							typErr.errStr = typErrOperKeyPass.errStr;
							result = typErr;
							break;
						}
						TypErr typErr3 = All.SF.SignatureFile(typErrOperKeyPass.KeyFile.Trim(), typErrOperKeyPass.Pass.Trim(), text8);
						if (typErr3.errCode > 0)
						{
							typErr.errCode = typErr3.errCode;
							typErr.errStr = typErr3.errStr;
							result = typErr;
							break;
						}
						string pathFile = text8;
						text8 += ".p7s";
						TypErrSubmit typErrSubmit = submitPtr.SubmitCheck(text8, CheckIDv, 1, dd, idcancel);
						if (typErrSubmit.errCode > 0)
						{
							typErr.errCode = typErrSubmit.errCode;
							typErr.errStr = typErrSubmit.errStr + "  Status: " + typErrSubmit.returnStatus + "  Msg: " + typErrSubmit.returnStr + "   ";
							result = typErr;
							break;
						}
						if (typErrSubmit.returnStatus < 0)
						{
							typErr.errCode = 26;
							typErr.errStr = "Отпралвяемый чек не принят сервером. Ответ  Status: " + typErrSubmit.returnStatus + "  Msg:" + typErrSubmit.returnStr + "   ";
							result = typErr;
							break;
						}
						if (typErrSubmit.returnStatus == 0)
						{
							typErr.errCode = 32;
							typErr.errStr = "Переход в офлайн режим";
							result = typErr;
							break;
						}
						CheckTaxNum = typErrSubmit.returnNumber;
						try
						{
							typErrStr3.ReturnStr = TaxDot(typErrStr3.ReturnStr);
							int num28 = Conversions.ToInteger(All.l.MaxID("CHECKHEAD").ReturnStr) + 1;
							typErr = All.l.SaveXMLcheck(num28.ToString(), typErrStr3.ReturnStr, xTAX, typErrSubmit.returnStr, typErrSubmit.returnNumber, opertyp, All.Bablo(num6.ToString()), pathFile);
							if (typErr.errCode > 0)
							{
								result = typErr;
								break;
							}
							typErr = All.l.SaveCheck(uid, num6.ToString(), opertyp, typErrSubmit.returnNumber, smbS);
							if (typErr.errCode > 0)
							{
								result = typErr;
								break;
							}
							XmlNodeList elementsByTagName5 = x.GetElementsByTagName("payment");
							int num29 = elementsByTagName5.Count - 1;
							num3 = 0;
							while (true)
							{
								if (num3 <= num29)
								{
									TypErrStr parametrToString4 = GetParametrToString(elementsByTagName5[num3].OuterXml, "id", "payment");
									if (parametrToString4.errCode > 0)
									{
										typErr.errCode = parametrToString4.errCode;
										typErr.errStr = parametrToString4.errStr;
										result = typErr;
										goto end_IL_034f;
									}
									TypErrStr parametrToString5 = GetParametrToString(elementsByTagName5[num3].OuterXml, "sum", "payment");
									if (parametrToString5.errCode > 0)
									{
										typErr.errCode = parametrToString5.errCode;
										typErr.errStr = parametrToString5.errStr;
										result = typErr;
										goto end_IL_034f;
									}
									if (Operators.CompareString(parametrToString4.ReturnStr, "1", TextCompare: false) == 0)
									{
										typErr = ((!(num11 > 0.0)) ? All.l.SaveCheckPay(num28.ToString(), All.PayTax.get_PayName(Conversions.ToInteger(parametrToString4.ReturnStr)), parametrToString5.ReturnStr) : All.l.SaveCheckPay(num28.ToString(), All.PayTax.get_PayName(Conversions.ToInteger(parametrToString4.ReturnStr)), num12.ToString()));
										goto IL_232e;
									}
									typErr = All.l.SaveCheckPay(num28.ToString(), All.PayTax.get_PayName(Conversions.ToInteger(parametrToString4.ReturnStr)), parametrToString5.ReturnStr);
									if (typErr.errCode <= 0)
									{
										goto IL_232e;
									}
									result = typErr;
									goto end_IL_034f;
								}
								if (Conversions.ToInteger(opertyp) == -8)
								{
									break;
								}
								Directorys payTax4 = All.PayTax;
								int taxN3 = payTax4.TaxN;
								num3 = 1;
								while (true)
								{
									if (num3 <= taxN3)
									{
										if (!(payTax4.get_Sum(num3) > 0.0))
										{
											goto IL_23d5;
										}
										float num30 = (float)payTax4.get_SumTax(num3);
										typErr = All.l.SaveTaxa(num28.ToString(), payTax4.get_TaxName(num3), payTax4.get_TaxPRC(num3), All.Bablo(num30.ToString()));
										if (typErr.errCode <= 0)
										{
											goto IL_23d5;
										}
										result = typErr;
										goto end_IL_034f;
									}
									payTax4 = null;
									int num31 = num;
									for (num3 = 0; num3 <= num31; num3++)
									{
										typErr = All.l.SaveGood(num28.ToString(), array[num3, 3], array[num3, 7], array[num3, 4], array[num3, 0], array[num3, 1], array[num3, 5], array[num3, 2]);
										if (typErr.errCode > 0)
										{
											result = typErr;
											goto end_IL_034f;
										}
									}
									break;
									IL_23d5:
									num3++;
								}
								break;
								IL_232e:
								if (typErr.errCode > 0)
								{
									result = typErr;
									goto end_IL_034f;
								}
								num3++;
							}
							goto IL_2486;
						}
						catch (Exception ex7)
						{
							ProjectData.SetProjectError(ex7);
							Exception ex8 = ex7;
							typErr.errStr = "Ошибка записи товаров в таблицу";
							typErr.errCode = 14;
							result = typErr;
							ProjectData.ClearProjectError();
						}
						break;
						IL_1e48:
						result = typErr;
						break;
						IL_2486:
						result = typErr;
						break;
						continue;
						end_IL_034f:
						break;
					}
					break;
				}
				break;
			}
			return result;
		}
	}

	private double Okruglit(double m, double smbO = 0.0)
	{
		m = All.StrToDouble(Strings.FormatNumber(m, 1));
		if (smbO == 0.0)
		{
			return m;
		}
		string text = All.Bablo(smbO);
		string left = text[checked(text.Length - 2)].ToString();
		if ((Operators.CompareString(left, "0", TextCompare: false) == 0) | (Operators.CompareString(left, "5", TextCompare: false) == 0))
		{
			return Okruglit5(m);
		}
		return m;
	}

	private double Okruglit5(double m)
	{
		string text = All.Bablo(m);
		checked
		{
			int num = Conversions.ToInteger(text[text.Length - 2].ToString());
			int num2 = 0;
			if (num > 7)
			{
				num2 = 10 - num;
				m += (double)num2 / 10.0;
			}
			else if (num < 3)
			{
				num2 = num;
				m -= (double)num2 / 10.0;
			}
			else
			{
				num2 = 5 - num;
				m += (double)num2 / 10.0;
			}
			return All.StrToDouble(All.Bablo(m));
		}
	}

	internal string AAAAA(string aaaS)
	{
		aaaS = aaaS.Trim();
		if (Operators.CompareString(aaaS, "", TextCompare: false) == 0)
		{
			return "'/>";
		}
		string text = "'>";
		checked
		{
			try
			{
				Array array = aaaS.Split(',');
				int num = array.Length - 1;
				for (int i = 0; i <= num; i++)
				{
					string text2 = Conversions.ToString(NewLateBinding.LateIndexGet(array, new object[1] { i }, null));
					text = text + "<CA CA='" + text2.Trim() + "'></CA>";
				}
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				text = text + "<CA>" + aaaS + "</CA>";
				ProjectData.ClearProjectError();
			}
			return text + "</P>";
		}
	}

	private double Discount(string n, string c, string s)
	{
		double num = All.StrToDouble(n);
		double num2 = All.StrToDouble(c);
		double num3 = All.StrToDouble(s);
		double num4 = All.StrToDouble(All.Bablo((num * num2).ToString()));
		if (num3 < num4)
		{
			return num4 - num3;
		}
		if (num3 > num4)
		{
			return num4 - num3;
		}
		return 0.0;
	}

	private TypErr Dereban(string s)
	{
		TypErr typErr = default(TypErr);
		typErr.errCode = 0;
		typErr.errStr = "";
		XmlDocument xmlDocument = new XmlDocument();
		xmlDocument.LoadXml(s);
		string text;
		try
		{
			text = xmlDocument.SelectSingleNode("/good/@quantity").Value;
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			text = "1";
			ProjectData.ClearProjectError();
		}
		string text2 = text;
		TypErr result;
		if (All.StrToDouble(text2) < 0.0)
		{
			typErr.errCode = 1017;
			typErr.errStr = "Помилка. Число може бути негативним.";
			result = typErr;
		}
		else
		{
			try
			{
				text = xmlDocument.SelectSingleNode("/good/@price").Value;
				if (All.StrToDouble(text.Trim()) <= 0.0)
				{
					typErr.errStr = "Помилка при обробці товарів у чеку, перевірте ціни.";
					typErr.errCode = 12;
					result = typErr;
				}
				else
				{
					string text3 = text;
					text = xmlDocument.SelectSingleNode("/good/@sum").Value;
					if (All.StrToDouble(text.Trim()) <= 0.0)
					{
						typErr.errStr = "Помилка при обробці товарів у чеку, перевірте суми.";
						typErr.errCode = 12;
						result = typErr;
					}
					else
					{
						string text4 = text;
						text = xmlDocument.SelectSingleNode("/good/@code").Value;
						string text5 = text;
						text = xmlDocument.SelectSingleNode("/good/@name").Value;
						if (Operators.CompareString(text.Trim(), "", TextCompare: false) == 0)
						{
							typErr.errStr = "Помилка при обробці товарів у чеку, перевірте назву товарів.";
							typErr.errCode = 12;
							result = typErr;
						}
						else
						{
							text = Strings.Replace(text, "'", "");
							text = Strings.Replace(text, "\"", "");
							string text6 = text;
							text = xmlDocument.SelectSingleNode("/good/@taxrate").Value;
							string text7 = text.ToUpper();
							if (Versioned.IsNumeric(text7))
							{
								text7 = All.PayTax.NUMtoABC(text7);
							}
							else
							{
								text7 = All.PayTax.ABCtoNUM(text7);
								text7 = All.PayTax.NUMtoABC(text7);
							}
							try
							{
								text = xmlDocument.SelectSingleNode("/good/@uktzed").Value;
							}
							catch (Exception ex3)
							{
								ProjectData.SetProjectError(ex3);
								Exception ex4 = ex3;
								text = "";
								ProjectData.ClearProjectError();
							}
							string text8 = text;
							try
							{
								text = xmlDocument.SelectSingleNode("/good/@excisestamp").Value;
							}
							catch (Exception ex5)
							{
								ProjectData.SetProjectError(ex5);
								Exception ex6 = ex5;
								text = "";
								ProjectData.ClearProjectError();
							}
							string text9 = text;
							try
							{
								text = xmlDocument.SelectSingleNode("/good/@barcode").Value;
							}
							catch (Exception ex7)
							{
								ProjectData.SetProjectError(ex7);
								Exception ex8 = ex7;
								text = "";
								ProjectData.ClearProjectError();
							}
							string text10 = text;
							try
							{
								text = xmlDocument.SelectSingleNode("/good/@avans").Value;
							}
							catch (Exception ex9)
							{
								ProjectData.SetProjectError(ex9);
								Exception ex10 = ex9;
								text = "";
								ProjectData.ClearProjectError();
							}
							string text11 = text;
							try
							{
								text = xmlDocument.SelectSingleNode("/good/@avansm").Value;
							}
							catch (Exception ex11)
							{
								ProjectData.SetProjectError(ex11);
								Exception ex12 = ex11;
								text = "аванс";
								ProjectData.ClearProjectError();
							}
							string text12 = text;
							t[0] = text2;
							t[1] = text3;
							t[2] = text4;
							t[3] = text5;
							t[4] = text6;
							t[5] = text7;
							t[6] = "0";
							t[7] = text8;
							t[8] = text9;
							t[9] = text10;
							t[10] = text11;
							t[11] = text12;
							string text13 = t[4];
							if ((Operators.CompareString(t[7], "", TextCompare: false) != 0) | (Operators.CompareString(t[8], "", TextCompare: false) != 0))
							{
								text13 = text13 + "~`~" + t[7] + "~`~" + t[8];
							}
							t[4] = text13;
							switch (text7)
							{
							case "ГА":
							{
								t[6] = All.TaxAmountBig(All.StrToDouble(t[2]), 5.0).ToString();
								All.PayTax.get_Sum(4) += All.StrToDouble(t[2]);
								All.PayTax.get_SumTax(4) += All.StrToDouble(t[6]);
								double num3 = All.StrToDouble(t[2]) - All.StrToDouble(t[6]);
								double num4 = All.TaxAmountBig(All.StrToDouble(num3.ToString()), 20.0);
								All.PayTax.get_TXSM(4) += num4;
								All.PayTax.get_Sum(1) += num3;
								All.PayTax.get_SumTax(1) += num4;
								goto end_IL_007e;
							}
							case "ГБ":
							{
								t[6] = All.TaxAmountBig(All.StrToDouble(t[2]), 5.0).ToString();
								All.PayTax.get_Sum(5) += All.StrToDouble(t[2]);
								All.PayTax.get_SumTax(5) += All.StrToDouble(t[6]);
								double num = All.StrToDouble(t[2]) - All.StrToDouble(t[6]);
								double num2 = 0.0;
								All.PayTax.get_TXSM(5) += num2;
								All.PayTax.get_Sum(2) += num;
								All.PayTax.get_SumTax(2) += num2;
								goto end_IL_007e;
							}
							case "ДА":
							{
								double num5 = All.TaxAmountBig(All.StrToDouble(t[2]), 27.5);
								double num6 = All.StrToDouble(t[2]) - num5;
								t[6] = All.TaxAmountr(num6, 7.5).ToString();
								All.PayTax.get_Sum(6) += All.StrToDouble(t[2]);
								All.PayTax.get_SumTax(6) += All.StrToDouble(t[6]);
								double num7 = num6 + All.TaxAmountr(num6, 20.0);
								double num8 = All.TaxAmountr(num6, 20.0);
								All.PayTax.get_TXSM(6) += num8;
								All.PayTax.get_Sum(1) += num7;
								All.PayTax.get_SumTax(1) += num8;
								goto end_IL_007e;
							}
							case "ДБ":
								typErr.errStr = "Ошибка! Налога ДБ быть не может.";
								typErr.errCode = 76;
								result = typErr;
								break;
							default:
							{
								TypErrTaxABC typErrTaxABC = All.PayTax.Search(text7);
								if (typErrTaxABC.errCode > 0)
								{
									typErr.errCode = typErrTaxABC.errCode;
									typErr.errStr = typErrTaxABC.errStr;
									result = typErr;
									break;
								}
								t[6] = All.TaxAmountBig(All.StrToDouble(t[2]), typErrTaxABC.TaxPrc).ToString();
								All.PayTax.get_Sum(typErrTaxABC.TaxIndex) += All.StrToDouble(t[2]);
								All.PayTax.get_SumTax(typErrTaxABC.TaxIndex) += All.StrToDouble(t[6]);
								goto end_IL_007e;
							}
							}
						}
					}
				}
				goto IL_07f1;
				end_IL_007e:;
			}
			catch (Exception ex13)
			{
				ProjectData.SetProjectError(ex13);
				Exception ex14 = ex13;
				typErr.errStr = "Помилка при обробці товарів у чеку.";
				typErr.errCode = 12;
				result = typErr;
				ProjectData.ClearProjectError();
				goto IL_07f1;
			}
			result = typErr;
		}
		goto IL_07f1;
		IL_07f1:
		return result;
	}

	public TypErrStr GetParametrToString(string sXML, string name, string knot = "InputParameters/Parameters", bool RegUpLow = false)
	{
		TypErrStr parametrToStringSE = default(TypErrStr);
		parametrToStringSE.errCode = 0;
		parametrToStringSE.errStr = "";
		parametrToStringSE.ReturnStr = "";
		try
		{
			XmlDocument xmlDocument = new XmlDocument();
			if (!RegUpLow)
			{
				sXML = sXML.ToLower();
				name = name.ToLower();
				knot = knot.ToLower();
			}
			xmlDocument.LoadXml(sXML.Trim());
			name = name.Trim();
			knot = knot.Trim();
			if (!Information.IsNothing(xmlDocument.SelectSingleNode("/" + knot + "/@" + name)))
			{
				parametrToStringSE.ReturnStr = xmlDocument.SelectSingleNode("/" + knot + "/@" + name).Value;
			}
			else
			{
				parametrToStringSE = GetParametrToStringSE(sXML, name, knot, RegUpLow);
			}
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			parametrToStringSE = GetParametrToStringSE(sXML, name, knot, RegUpLow);
			ProjectData.ClearProjectError();
		}
		return parametrToStringSE;
	}

	private TypErrStr GetParametrToStringSE(string sXML, string name, string knot = "InputParameters/Parameters", bool RegUpLow = false)
	{
		TypErrStr result = default(TypErrStr);
		result.errCode = 0;
		result.errStr = "";
		result.ReturnStr = "";
		try
		{
			XmlDocument xmlDocument = new XmlDocument();
			if (!RegUpLow)
			{
				sXML = sXML.ToLower();
				name = name.ToLower();
				knot = knot.ToLower();
			}
			xmlDocument.LoadXml(sXML.Trim());
			name = name.Trim().ToLower();
			knot = knot.Trim().ToLower();
			if (!Information.IsNothing(xmlDocument.SelectSingleNode("/" + knot + "/@" + name)))
			{
				result.ReturnStr = xmlDocument.SelectSingleNode("/" + knot + "/@" + name).Value;
			}
			else
			{
				result.ReturnStr = "";
				result.errCode = 1004;
				result.errStr = "Неверный формат XML.";
			}
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result.ReturnStr = "";
			result.errCode = 1004;
			result.errStr = "Неверный формат XML.";
			ProjectData.ClearProjectError();
		}
		return result;
	}

	internal string TegXml(string xmlOld)
	{
		string text = xmlOld.Trim();
		if (Operators.CompareString(text, "", TextCompare: false) == 0)
		{
			return "";
		}
		int num = 0;
		checked
		{
			do
			{
				text = Strings.Replace(text, " =", "=");
				text = Strings.Replace(text, "= ", "=");
				num++;
			}
			while (num <= 2);
			while (Strings.InStr(text, "=") > 0)
			{
				int num2 = Strings.InStr(text, "=") - 1;
				string text2 = "=";
				try
				{
					while (Operators.CompareString(Conversions.ToString(text[num2]), " ", TextCompare: false) != 0)
					{
						if (Operators.CompareString(Conversions.ToString(text[num2]), "ё", TextCompare: false) == 0)
						{
							text2 = "";
							break;
						}
						num2--;
						text2 = Conversions.ToString(text[num2]) + text2;
					}
				}
				catch (Exception ex)
				{
					ProjectData.SetProjectError(ex);
					Exception ex2 = ex;
					ProjectData.ClearProjectError();
				}
				if (Operators.CompareString(text2, "", TextCompare: false) != 0)
				{
					string replacement = text2.ToLower();
					text = Strings.Replace(text, text2, replacement);
				}
				text = Strings.Replace(text, "=", "ёёё", 1, 1);
			}
			text = Strings.Replace(text, "ёёё", "=");
			text = KnotBeginXml(text);
			return KnotEndXml(text);
		}
	}

	private string KnotBeginXml(string xmlOld)
	{
		string text = xmlOld;
		int num = 0;
		checked
		{
			do
			{
				text = Strings.Replace(text, "< ", "<");
				num++;
			}
			while (num <= 2);
			while (Strings.InStr(text, "<") > 0)
			{
				int num2 = Strings.InStr(text, "<") - 1;
				string text2 = "<";
				try
				{
					while (Operators.CompareString(Conversions.ToString(text[num2]), " ", TextCompare: false) != 0 && Operators.CompareString(Conversions.ToString(text[num2]), ">", TextCompare: false) != 0)
					{
						num2++;
						text2 += Conversions.ToString(text[num2]);
					}
				}
				catch (Exception ex)
				{
					ProjectData.SetProjectError(ex);
					Exception ex2 = ex;
					ProjectData.ClearProjectError();
				}
				string replacement = text2.ToLower();
				text = Strings.Replace(text, text2, replacement);
				text = Strings.Replace(text, "<", "ёёё", 1, 1);
			}
			return Strings.Replace(text, "ёёё", "<");
		}
	}

	private string KnotEndXml(string xmlOld)
	{
		string text = xmlOld;
		int num = 0;
		checked
		{
			do
			{
				text = Strings.Replace(text, "/ ", "/");
				text = Strings.Replace(text, "< ", "<");
				num++;
			}
			while (num <= 2);
			while (Strings.InStr(text, "</") > 0)
			{
				int num2 = Strings.InStr(text, "</");
				string text2 = "</";
				try
				{
					while (Operators.CompareString(Conversions.ToString(text[num2]), ">", TextCompare: false) != 0 && Operators.CompareString(Conversions.ToString(text[num2]), " ", TextCompare: false) != 0)
					{
						num2++;
						text2 += Conversions.ToString(text[num2]);
					}
				}
				catch (Exception ex)
				{
					ProjectData.SetProjectError(ex);
					Exception ex2 = ex;
					ProjectData.ClearProjectError();
				}
				string replacement = text2.ToLower();
				text = Strings.Replace(text, text2, replacement);
				text = Strings.Replace(text, "/", "ёёё", 1, 1);
			}
			return Strings.Replace(text, "ёёё", "/");
		}
	}

	public TypErrStr GetParametrToXML(string s, bool zamena = true)
	{
		TypErrStr typErrStr = default(TypErrStr);
		typErrStr.errCode = 0;
		typErrStr.errStr = "";
		typErrStr.ReturnStr = "";
		TypErrStr result;
		checked
		{
			try
			{
				string text = "<OutputParameters><Parameters";
				Array array = s.Split('_');
				int num = array.GetLength(0) - 1;
				for (int i = 0; i <= num; i++)
				{
					Array instance = (Array)NewLateBinding.LateGet(NewLateBinding.LateIndexGet(array, new object[1] { i }, null), null, "Split", new object[1] { "=" }, null, null, null);
					text = Conversions.ToString(Operators.AddObject(Operators.AddObject(Operators.AddObject(Operators.AddObject(text + " ", NewLateBinding.LateIndexGet(instance, new object[1] { 0 }, null)), "='"), NewLateBinding.LateIndexGet(instance, new object[1] { 1 }, null)), "'"));
				}
				text += "/></OutputParameters>";
				text = text.Replace("`", "_");
				if (zamena)
				{
					typErrStr.ReturnStr = Strings.Replace(text, "'", '"'.ToString());
				}
				else
				{
					typErrStr.ReturnStr = text;
				}
				new XmlDocument().LoadXml(typErrStr.ReturnStr.Trim().ToLower());
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				All.Lg.SaveTextToLog("GetParametrToXML  err: " + ex2.Message, s, typErrStr.ReturnStr);
				typErrStr.errCode = 1005;
				typErrStr.errStr = "Сформовано невірний формат XML " + ex2.Message;
				typErrStr.ReturnStr = "Помилка формування відповіді від ПРРО " + ex2.Message;
				result = typErrStr;
				ProjectData.ClearProjectError();
				goto IL_01c8;
			}
			result = typErrStr;
			goto IL_01c8;
		}
		IL_01c8:
		return result;
	}

	public TypErrStr GetParametrToXMLterminal(string s, bool zamena = true)
	{
		TypErrStr typErrStr = default(TypErrStr);
		typErrStr.errCode = 0;
		typErrStr.errStr = "";
		typErrStr.ReturnStr = "";
		TypErrStr result;
		checked
		{
			try
			{
				string text = "<OutputParameters><Parameters";
				Array array = s.Split('_');
				int num = array.GetLength(0) - 1;
				for (int i = 0; i <= num; i++)
				{
					Array instance = (Array)NewLateBinding.LateGet(NewLateBinding.LateIndexGet(array, new object[1] { i }, null), null, "Split", new object[1] { "=" }, null, null, null);
					text = Conversions.ToString(Operators.AddObject(Operators.AddObject(Operators.AddObject(Operators.AddObject(text + " ", NewLateBinding.LateIndexGet(instance, new object[1] { 0 }, null)), "='"), NewLateBinding.LateIndexGet(instance, new object[1] { 1 }, null)), "'"));
				}
				text += "/></OutputParameters>";
				text = text.Replace("`", "_");
				text = text.Replace("в'я", "вя");
				text = text.Replace("\"", "");
				if (zamena)
				{
					typErrStr.ReturnStr = Strings.Replace(text, "'", '"'.ToString());
				}
				else
				{
					typErrStr.ReturnStr = text;
				}
				new XmlDocument().LoadXml(typErrStr.ReturnStr.Trim().ToLower());
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				All.LgT.SaveTextToLogCardserv("GetParametrToXML  err: " + ex2.Message, s, typErrStr.ReturnStr);
				typErrStr.errCode = 1005;
				typErrStr.errStr = "Сформовано невірний формат XML " + ex2.Message;
				typErrStr.ReturnStr = "Помилка формування відповіді від банку " + ex2.Message;
				result = typErrStr;
				ProjectData.ClearProjectError();
				goto IL_01ea;
			}
			result = typErrStr;
			goto IL_01ea;
		}
		IL_01ea:
		return result;
	}

	public TypErrStr ReportXZ(string sh, string tp = "x", string NumShift = "")
	{
		CheckTaxNum = "";
		CheckIDv = "";
		if (Operators.CompareString(tp, "x", TextCompare: false) == 0)
		{
			NumberShift = sh;
		}
		else
		{
			NumberShift = NumShift;
		}
		TypErrStr typErrStr = default(TypErrStr);
		typErrStr.errCode = 0;
		typErrStr.errStr = "";
		typErrStr.ReturnStr = "";
		TypErr typErr = default(TypErr);
		typErr.errCode = 0;
		typErr.errStr = "";
		All.A.Report = "";
		All.PayTax.ZeroReport();
		checked
		{
			int num = Conversions.ToInteger(All.l.MaxID("ksef").ReturnStr) + 1;
			TypSMB typSMB = All.Rf.ReportSMB(sh);
			typErr = All.Rf.Reprt1(sh);
			TypErrStr result;
			if (typErr.errCode > 0)
			{
				typErrStr.errCode = typErr.errCode;
				typErrStr.errStr = typErr.errStr;
				result = typErrStr;
			}
			else
			{
				long dd = All.СurrentCompDate();
				ZX = "";
				string text = All.A.TIN;
				if (Versioned.IsNumeric(All.A.INN) && Convert.ToDouble(All.A.INN) > 0.0)
				{
					text = All.A.INN;
				}
				ref string zX = ref ZX;
				ref string reference = ref zX;
				zX = reference + "<DAT FN='" + All.A.FN + "' TN='" + text + "' DI='" + num + "' ZN='0' V='1'>";
				ref string zX2 = ref ZX;
				zX2 = zX2 + "<Z NO='" + sh + "'>";
				Directorys payTax = All.PayTax;
				_ = payTax.TaxN;
				int taxN = payTax.TaxN;
				int num2 = 1;
				SubmitPtr submitPtr = default(SubmitPtr);
				while (true)
				{
					if (num2 <= taxN)
					{
						if (Operators.CompareString(payTax.get_ReportsSum(0, num2), "", TextCompare: false) != 0)
						{
							string text2 = payTax.ABCtoNUM(payTax.get_ReportsSum(0, num2));
							DateTime now = DateTime.Now;
							string text3 = now.Year + All.TwoS(now.Month.ToString()) + All.TwoS(now.Day.ToString());
							if (payTax.get_ReportsSum(0, num2).Trim().Length == 1)
							{
								if (!Versioned.IsNumeric(payTax.get_ReportsSum(4, num2)))
								{
									payTax.set_ReportsSum(4, num2, "0");
								}
								if (!Versioned.IsNumeric(payTax.get_ReportsSum(1, num2)))
								{
									payTax.set_ReportsSum(1, num2, "0");
								}
								if (!Versioned.IsNumeric(payTax.get_ReportsSum(5, num2)))
								{
									payTax.set_ReportsSum(5, num2, "0");
								}
								TypErrOborot typErrOborot = All.Rf.Report6(sh);
								if (typErrOborot.errCode > 0)
								{
									typErrStr.errCode = typErrOborot.errCode;
									typErrStr.errStr = typErrOborot.errStr;
									typErrStr.ReturnStr = "";
									result = typErrStr;
									break;
								}
								double num3 = 0.0;
								double num4 = 0.0;
								double num5 = 0.0;
								double num6 = 0.0;
								double num7 = 0.0;
								if (Versioned.IsNumeric(typErrOborot.SumTXA))
								{
									num5 = All.StrToDouble(typErrOborot.SumTXA);
								}
								if (Versioned.IsNumeric(typErrOborot.SumTXGA))
								{
									num6 = All.StrToDouble(typErrOborot.SumTXGA);
									num5 += num6 - num6 * 5.0 / 105.0;
								}
								if (Versioned.IsNumeric(typErrOborot.SumTXDA))
								{
									num7 = All.StrToDouble(typErrOborot.SumTXDA);
									num5 += num7 / 1.275 * 1.2;
								}
								num3 = num5 / 6.0;
								num5 = 0.0;
								num6 = 0.0;
								num7 = 0.0;
								if (Versioned.IsNumeric(typErrOborot.SumTXAret))
								{
									num5 = All.StrToDouble(typErrOborot.SumTXAret);
								}
								if (Versioned.IsNumeric(typErrOborot.SumTXGAret))
								{
									num6 = All.StrToDouble(typErrOborot.SumTXGAret);
									num5 += num6 - num6 * 5.0 / 105.0;
								}
								if (Versioned.IsNumeric(typErrOborot.SumTXDAret))
								{
									num7 = All.StrToDouble(typErrOborot.SumTXDAret);
									num5 += num7 / 1.275 * 1.2;
								}
								num4 = num5 / 6.0;
								string sBablo = num3.ToString();
								string sBablo2 = num4.ToString();
								payTax.set_ReportsSum(3, num2, All.TaxAmountBig(All.StrToDouble(payTax.get_ReportsSum(4, num2)), All.StrToDouble(payTax.get_ReportsSum(1, num2))).ToString());
								payTax.set_ReportsSum(2, num2, All.TaxAmountBig(All.StrToDouble(payTax.get_ReportsSum(5, num2)), All.StrToDouble(payTax.get_ReportsSum(1, num2))).ToString());
								ref string zX3 = ref ZX;
								reference = ref zX3;
								zX3 = reference + "<TXS TX='" + text2 + "' wchkain='" + All.Bablo(sBablo) + "' wchkaout='" + All.Bablo(sBablo2) + "' TS='" + text3 + "' N='" + payTax.get_ReportsSum(0, num2) + "' TXPR='" + TaxDot(All.Bablo(payTax.get_ReportsSum(1, num2)), dot: false) + "' TXI='" + All.Bablo(payTax.get_ReportsSum(3, num2)) + "' TXO='" + All.Bablo(payTax.get_ReportsSum(2, num2)) + "' SMI='" + All.Bablo(payTax.get_ReportsSum(4, num2)) + "' SMO='" + All.Bablo(payTax.get_ReportsSum(5, num2)) + "' DTPR='" + TaxDot("0.00", dot: false) + "' DTI='0.00' DTO='0' TXTY='0' TXAL='0'/>";
							}
							else
							{
								double num8 = ((!Versioned.IsNumeric(payTax.get_ReportsSum(4, num2))) ? 0.0 : All.StrToDouble(payTax.get_ReportsSum(4, num2)));
								if (Versioned.IsNumeric(payTax.get_ReportsSum(3, num2)))
								{
									double num9 = All.StrToDouble(payTax.get_ReportsSum(3, num2));
								}
								else
								{
									double num9 = 0.0;
								}
								double num10 = ((!Versioned.IsNumeric(payTax.get_ReportsSum(5, num2))) ? 0.0 : All.StrToDouble(payTax.get_ReportsSum(5, num2)));
								double num11 = ((!Versioned.IsNumeric(payTax.get_ReportsSum(2, num2))) ? 0.0 : All.StrToDouble(payTax.get_ReportsSum(2, num2)));
								switch (text2)
								{
								case "4":
								{
									double num9 = num8 * 5.0 / 105.0;
									double num18 = (num8 - num9) / 6.0;
									double num19 = (num10 - num11) / 6.0;
									ref string zX7 = ref ZX;
									reference = ref zX7;
									zX7 = reference + "<TXS TX='" + text2 + "' TS='" + text3 + "' N='" + payTax.get_ReportsSum(0, num2) + "' TXPR='" + TaxDot("20.00", dot: false) + "' TXI='" + All.Bablo(num18.ToString()) + "' TXO='" + All.Bablo(num19.ToString()) + "' SMI='" + All.Bablo(payTax.get_ReportsSum(4, num2)) + "' SMO='" + All.Bablo(payTax.get_ReportsSum(5, num2)) + "' DTPR='" + TaxDot("5.00", dot: false) + "' DTI='" + All.Bablo(num9.ToString()) + "' DTO='" + All.Bablo(payTax.get_ReportsSum(2, num2)) + "' TXTY='0' TXAL='2'/>";
									break;
								}
								case "5":
								{
									double num9 = num8 * 5.0 / 105.0;
									double num16 = 0.0;
									double num17 = 0.0;
									ref string zX6 = ref ZX;
									reference = ref zX6;
									zX6 = reference + "<TXS TX='" + text2 + "' TS='" + text3 + "' N='" + payTax.get_ReportsSum(0, num2) + "' TXPR='" + TaxDot("0.00", dot: false) + "' TXI='" + All.Bablo(num16.ToString()) + "' TXO='" + All.Bablo(num17.ToString()) + "' SMI='" + All.Bablo(payTax.get_ReportsSum(4, num2)) + "' SMO='" + All.Bablo(payTax.get_ReportsSum(5, num2)) + "' DTPR='" + TaxDot("5.00", dot: false) + "' DTI='" + All.Bablo(num9.ToString()) + "' DTO='" + All.Bablo(payTax.get_ReportsSum(2, num2)) + "' TXTY='0' TXAL='2'/>";
									break;
								}
								case "6":
								{
									double num9 = num8 / 1.275 * 0.075;
									double num14 = (num8 - num9) / 6.0;
									double num15 = (num10 - num11) / 6.0;
									ref string zX5 = ref ZX;
									reference = ref zX5;
									zX5 = reference + "<TXS TX='" + text2 + "' TS='" + text3 + "' N='" + payTax.get_ReportsSum(0, num2) + "' TXPR='" + TaxDot("20.00", dot: false) + "' TXI='" + All.Bablo(num14.ToString()) + "' TXO='" + All.Bablo(num15.ToString()) + "' SMI='" + All.Bablo(payTax.get_ReportsSum(4, num2)) + "' SMO='" + All.Bablo(payTax.get_ReportsSum(5, num2)) + "' DTPR='" + TaxDot("7.50", dot: false) + "' DTI='" + All.Bablo(num9.ToString()) + "' DTO='" + All.Bablo(payTax.get_ReportsSum(2, num2)) + "' TXTY='0' TXAL='0'/>";
									break;
								}
								case "7":
								{
									double num9 = num8 / 1.275 * 0.075;
									double num12 = 0.0;
									double num13 = 0.0;
									ref string zX4 = ref ZX;
									reference = ref zX4;
									zX4 = reference + "<TXS TX='" + text2 + "' TS='" + text3 + "' N='" + payTax.get_ReportsSum(0, num2) + "' TXPR='" + TaxDot("0.00", dot: false) + "' TXI='" + All.Bablo(num12.ToString()) + "' TXO='" + All.Bablo(num13.ToString()) + "' SMI='" + All.Bablo(payTax.get_ReportsSum(4, num2)) + "' SMO='" + All.Bablo(payTax.get_ReportsSum(5, num2)) + "' DTPR='" + TaxDot("7.50", dot: false) + "' DTI='" + All.Bablo(num9.ToString()) + "' DTO='" + All.Bablo(payTax.get_ReportsSum(2, num2)) + "' TXTY='0' TXAL='0'/>";
									break;
								}
								}
							}
						}
						num2++;
						continue;
					}
					payTax = null;
					typErr = All.Rf.Reprt2(sh);
					if (typErr.errCode > 0)
					{
						typErrStr.errCode = typErr.errCode;
						typErrStr.errStr = typErr.errStr;
						result = typErrStr;
						break;
					}
					Directorys payTax2 = All.PayTax;
					int payN = payTax2.PayN;
					for (num2 = 1; num2 <= payN; num2++)
					{
						if (Operators.CompareString(payTax2.get_ReportsPay(0, num2), "", TextCompare: false) == 0)
						{
							continue;
						}
						string text4 = All.PayTax.get_PayISCASH(All.PayTax.get_PayABCtoNum(payTax2.get_ReportsPay(0, num2)));
						string text5 = "";
						if (Operators.CompareString(text4, "0", TextCompare: false) == 0)
						{
							if (typSMB.SMB)
							{
								text5 = "' SMIM='" + All.Bablo(typSMB.SMIM) + "' SMIP='" + All.Bablo(typSMB.SMIP) + "' SMOM='" + All.Bablo(typSMB.SMOM) + "' SMOP='" + All.Bablo(typSMB.SMOP);
							}
						}
						else
						{
							text5 = "";
						}
						ref string zX8 = ref ZX;
						reference = ref zX8;
						zX8 = reference + "<M T='" + text4 + "' NM='" + payTax2.get_ReportsPay(0, num2) + "' SMI='" + All.Bablo(payTax2.get_ReportsPay(1, num2)) + "' SMO='" + All.Bablo(payTax2.get_ReportsPay(2, num2)) + text5 + "'/>";
					}
					payTax2 = null;
					typErr = All.Rf.Reprt3(sh);
					if (typErr.errCode > 0)
					{
						typErrStr.errCode = typErr.errCode;
						typErrStr.errStr = typErr.errStr;
						result = typErrStr;
						break;
					}
					ref string zX9 = ref ZX;
					reference = ref zX9;
					zX9 = reference + "<NC NI='" + All.PayTax.Ni + "' NO='" + All.PayTax.No + "'/>";
					typErr = All.Rf.Reprt4(sh);
					if (typErr.errCode > 0)
					{
						typErrStr.errCode = typErr.errCode;
						typErrStr.errStr = typErr.errStr;
						result = typErrStr;
						break;
					}
					ref string zX10 = ref ZX;
					reference = ref zX10;
					zX10 = reference + "<IO  NM = 'ГОТІВКА' SMI='" + All.Bablo(All.PayTax.SMI) + "' SMO='" + All.Bablo(All.PayTax.SMO) + "'/>";
					TypErrEP typErrEP = All.Rf.Report7(sh);
					if (typErrEP.errCode > 0)
					{
						typErrStr.errCode = typErrEP.errCode;
						typErrStr.errStr = typErrEP.errStr;
						result = typErrStr;
						break;
					}
					ref string zX11 = ref ZX;
					reference = ref zX11;
					zX11 = reference + "<EPZ EPC='" + typErrEP.EPC + "' EPCS='0' EPSM='" + typErrEP.EPSM + "'></EPZ>";
					ZX += "</Z>";
					ref string zX12 = ref ZX;
					zX12 = zX12 + "<TS>" + dd + "</TS>";
					ZX += "</DAT>";
					if (All.l.OfflineTrue() && Operators.CompareString(tp.ToLower().Trim(), "z", TextCompare: false) == 0)
					{
						ZX = "<RQ V='1'>" + ZX + "mmmaaaccc</RQ>";
						All.OfflineNum = "";
						TypErrStr typErrStr2 = new NumbersOfflineUse().OfflineID();
						if (typErrStr2.errCode > 0)
						{
							typErrStr.errCode = typErrStr2.errCode;
							typErrStr.errStr = typErrStr2.errStr;
							result = typErrStr;
							break;
						}
						All.OfflineNum = typErrStr2.ReturnStr;
						CheckTaxNum = typErrStr2.ReturnStr;
						if (All.l.CloseOffline10())
						{
							typErrStr.errStr = "Ошибка записи оффлайн чека, сервер налоговой уже закрыл оффлайн режим. Повторите попытку.";
							typErrStr.errCode = 84;
							result = typErrStr;
							break;
						}
						if (All.l.BagCloseOfflineShift())
						{
							typErrStr.errStr = "Помилка закриття зміни в оффлайн режимі , зробить пошук помилок для виправлення.";
							typErrStr.errCode = 104;
							result = typErrStr;
							break;
						}
						try
						{
							string expression = Strings.Replace(ZX, ".", "");
							expression = Strings.Replace(expression, "#`#", ".");
							ZX = Strings.Replace(ZX, "#`#", ".");
							TypErr typErr2 = All.l.SaveXMLcheckOffline(sh.ToString() + "Z", TaxDot(ZX), TaxDot(expression), "not", typErrStr2.ReturnStr, "80");
							if (typErr2.errCode > 0)
							{
								typErrStr.errCode = typErr2.errCode;
								typErrStr.errStr = typErr2.errStr;
								result = typErrStr;
								break;
							}
							DateTime now2 = DateTime.Now;
							if (!Directory.Exists(All.MyDoc() + "\\WebCheck\\Archive\\" + All.A.FN + "\\" + now2.Year + "\\"))
							{
								Directory.CreateDirectory(All.MyDoc() + "\\WebCheck\\Archive\\" + All.A.FN + "\\" + now2.Year + "\\");
							}
							string pathPDF = All.MyDoc() + "\\WebCheck\\Archive\\" + All.A.FN + "\\" + now2.Year + "\\" + sh + "Z.pdf";
							if (Operators.CompareString(All.A.FiscalMode, "cabinet.tax.gov.ua:9443", TextCompare: false) == 0)
							{
								pathPDF = All.MyDoc() + "\\WebCheck\\Archive\\" + All.A.FN + "\\_TS\\" + now2.Year + "\\" + sh + "Z.pdf";
							}
							new PrintExportCheck().ExportCheckToPDF(pathPDF, ZX, typErrStr2.ReturnStr);
						}
						catch (Exception ex)
						{
							ProjectData.SetProjectError(ex);
							Exception ex2 = ex;
							typErrStr.errCode = 18;
							typErrStr.errStr = "Ошибка формировании ZX отчетов офлайн.";
							result = typErrStr;
							ProjectData.ClearProjectError();
							break;
						}
						typErrStr.ReturnStr = TaxDot(ZX);
						result = typErrStr;
						break;
					}
					All.MacTempOld = "";
					TypErrStr typErrStr3 = NameDotNoDot(ZX, sDot: true);
					if (typErrStr3.errCode > 0)
					{
						typErrStr.errCode = typErrStr3.errCode;
						typErrStr.errStr = typErrStr3.errStr;
						result = typErrStr;
						break;
					}
					typErrStr3.ReturnStr = TaxDot(typErrStr3.ReturnStr);
					ZX = NameDotNoDot(ZX, sDot: false).ReturnStr;
					ZX = TaxDot(ZX);
					string text6 = All.MyDoc() + "\\WebCheck\\Temp\\" + All.A.FN + "\\" + sh + tp + ".xml";
					All.SaveToFileText(text6, ZX);
					if (Operators.CompareString(tp.ToLower().Trim(), "z", TextCompare: false) == 0)
					{
						if (Versioned.IsNumeric(NumShift.Trim()))
						{
							TypErrOperKeyPass typErrOperKeyPass = All.l.OperatorKeyPass(NumShift);
							if (typErrOperKeyPass.errCode > 0)
							{
								typErrStr.errCode = typErrOperKeyPass.errCode;
								typErrStr.errStr = typErrOperKeyPass.errStr;
								result = typErrStr;
								break;
							}
							TypErr typErr3 = All.SF.SignatureFile(typErrOperKeyPass.KeyFile.Trim(), typErrOperKeyPass.Pass.Trim(), text6);
							if (typErr3.errCode > 0)
							{
								typErrStr.errCode = typErr3.errCode;
								typErrStr.errStr = typErr3.errStr;
								typErrStr.ReturnStr = "";
								result = typErrStr;
								break;
							}
						}
						string pathFile = text6;
						text6 += ".p7s";
						CheckIDv = sh;
						TypErrSubmit typErrSubmit = submitPtr.SubmitCheck(text6, CheckIDv, 2, dd, "", "", OpenCloseShift: true);
						if (typErrSubmit.errCode > 0)
						{
							typErrStr.errCode = typErrSubmit.errCode;
							typErrStr.errStr = typErrSubmit.errStr + "  Status: " + typErrSubmit.returnStatus + "  Msg: " + typErrSubmit.returnStr + "   ";
							result = typErrStr;
							break;
						}
						if (typErrSubmit.returnStatus < 0)
						{
							typErrStr.errCode = 27;
							typErrStr.errStr = "Отпралвяемый Z отчет не принят сервером. Ответ  Status: " + typErrSubmit.returnStatus + "  Msg:" + typErrSubmit.returnStr + "   ";
							result = typErrStr;
							break;
						}
						if (typErrSubmit.returnStatus == 0)
						{
							typErrStr.errCode = 32;
							typErrStr.errStr = "Переход в офлайн режим";
							result = typErrStr;
							break;
						}
						CheckTaxNum = typErrSubmit.returnNumber;
						typErr = All.l.SaveXMLcheck(sh.ToString() + "Z", typErrStr3.ReturnStr, ZX, typErrSubmit.returnStr, typErrSubmit.returnNumber, "80", "0.00", pathFile);
						if (typErr.errCode > 0)
						{
							typErrStr.errCode = typErr.errCode;
							typErrStr.errStr = typErr.errStr;
							typErrStr.ReturnStr = "";
							result = typErrStr;
							break;
						}
						DateTime now3 = DateTime.Now;
						if (!Directory.Exists(All.MyDoc() + "\\WebCheck\\Archive\\" + All.A.FN + "\\" + now3.Year + "\\"))
						{
							Directory.CreateDirectory(All.MyDoc() + "\\WebCheck\\Archive\\" + All.A.FN + "\\" + now3.Year + "\\");
						}
						string pathPDF2 = All.MyDoc() + "\\WebCheck\\Archive\\" + All.A.FN + "\\" + now3.Year + "\\" + sh + "Z.pdf";
						if (Operators.CompareString(All.A.FiscalMode, "cabinet.tax.gov.ua:9443", TextCompare: false) == 0)
						{
							pathPDF2 = All.MyDoc() + "\\WebCheck\\Archive\\" + All.A.FN + "\\_TS\\" + now3.Year + "\\" + sh + "Z.pdf";
						}
						new PrintExportCheck().ExportCheckToPDF(pathPDF2, typErrStr3.ReturnStr, typErrSubmit.returnNumber);
					}
					typErrStr.ReturnStr = typErrStr3.ReturnStr;
					result = typErrStr;
					break;
				}
			}
			return result;
		}
	}

	public TypErrStr DealCheck(string sXML, string NumShift)
	{
		CheckTaxNum = "";
		CheckIDv = "";
		TypErrStr typErrStr = default(TypErrStr);
		typErrStr.errCode = 0;
		typErrStr.errStr = "";
		typErrStr.ReturnStr = "";
		NumberShift = NumShift;
		TypErrStr result;
		checked
		{
			try
			{
				TypErrStr parametrToString = All.d.GetParametrToString(sXML, "paymentid");
				if (Operators.CompareString(parametrToString.ReturnStr, "", TextCompare: false) == 0)
				{
					parametrToString.ReturnStr = "1";
				}
				parametrToString.ReturnStr = All.PayTax.get_PayName(Conversions.ToInteger(parametrToString.ReturnStr));
				TypErrStr typErrStr2 = All.l.MaxID("ksef");
				int num;
				long dd;
				string text;
				string text2;
				if (typErrStr2.errCode > 0)
				{
					typErrStr.errCode = typErrStr2.errCode;
					typErrStr.errStr = typErrStr2.errStr;
					typErrStr.ReturnStr = "";
					result = typErrStr;
				}
				else
				{
					num = Conversions.ToInteger(typErrStr2.ReturnStr) + 1;
					dd = All.СurrentCompDate();
					text = "";
					text2 = "";
					string text3 = All.A.TIN;
					if (Versioned.IsNumeric(All.A.INN) && Convert.ToDouble(All.A.INN) > 0.0)
					{
						text3 = All.A.INN;
					}
					text2 = text2 + "<DAT FN='" + All.A.FN + "' TN='" + text3 + "' DI='" + num + "' ZN='0' V='1'>";
					text2 += "<C T='2'>";
					typErrStr = All.d.GetParametrToString(sXML, "sumin");
					if (All.StrToDouble(typErrStr.ReturnStr) > 0.0)
					{
						typErrStr.ReturnStr = All.Bablo(typErrStr.ReturnStr);
						text2 = text2 + "<I N='1' T='0' SM='" + typErrStr.ReturnStr + "'/>";
						text = "3";
						goto IL_028a;
					}
					typErrStr = All.d.GetParametrToString(sXML, "sumout");
					if (!(All.StrToDouble(typErrStr.ReturnStr) > 0.0))
					{
						goto IL_028a;
					}
					if (!(All.StrToDouble(typErrStr.ReturnStr) > All.Nal()))
					{
						text2 = text2 + "<O N='1' T='0' SM='" + All.Bablo(typErrStr.ReturnStr) + "'/>";
						text = "4";
						goto IL_028a;
					}
					typErrStr.errCode = 47;
					typErrStr.errStr = "Помилка! У касі немає необхідної суми.";
					typErrStr.ReturnStr = "";
					result = typErrStr;
				}
				goto end_IL_003d;
				IL_028a:
				if (Operators.CompareString(text, "", TextCompare: false) == 0)
				{
					typErrStr.errCode = 20;
					typErrStr.errStr = "Тип чека містить помилку.";
					typErrStr.ReturnStr = "";
					result = typErrStr;
				}
				else
				{
					text2 += "<E N='2'/>";
					text2 += "</C>";
					text2 = text2 + "<TS>" + dd + "</TS>";
					text2 += "</DAT>";
					if (All.l.OfflineTrue())
					{
						text2 = "<RQ V='1'>" + text2 + "mmmaaaccc</RQ>";
						All.OfflineNum = "";
						TypErrStr typErrStr3 = new NumbersOfflineUse().OfflineID();
						if (typErrStr3.errCode > 0)
						{
							typErrStr.errCode = typErrStr3.errCode;
							typErrStr.errStr = typErrStr3.errStr;
							result = typErrStr;
						}
						else
						{
							All.OfflineNum = typErrStr3.ReturnStr;
							CheckTaxNum = typErrStr3.ReturnStr;
							if (All.l.CloseOffline10())
							{
								typErrStr.errStr = "Помилка запису офлайн чека, сервер податкової вже закрив офлайн режим. Повторіть спробу.";
								typErrStr.errCode = 84;
								result = typErrStr;
							}
							else if (All.l.BagCloseOfflineShift())
							{
								typErrStr.errStr = "Помилка закриття зміни в оффлайн режимі , зробить пошук помилок для виправлення.";
								typErrStr.errCode = 104;
								result = typErrStr;
							}
							else
							{
								try
								{
									string expression = Strings.Replace(text2, ".", "");
									expression = Strings.Replace(expression, "#`#", ".");
									text2 = Strings.Replace(text2, "#`#", ".");
									int num2 = Conversions.ToInteger(All.l.MaxID("CHECKHEAD").ReturnStr) + 1;
									TypErr typErr = All.l.SaveXMLcheckOffline(num2.ToString(), text2, expression, "not", typErrStr3.ReturnStr, text, All.Bablo(typErrStr.ReturnStr));
									if (typErr.errCode > 0)
									{
										typErrStr.errCode = typErr.errCode;
										typErrStr.errStr = typErr.errStr;
										result = typErrStr;
									}
									else
									{
										typErrStr = All.l.SaveDealCheck(typErrStr.ReturnStr, text, parametrToString.ReturnStr, typErrStr3.ReturnStr);
										if (typErrStr.errCode <= 0)
										{
											goto end_IL_03d4;
										}
										result = typErrStr;
									}
									goto end_IL_003d;
									end_IL_03d4:;
								}
								catch (Exception ex)
								{
									ProjectData.SetProjectError(ex);
									Exception ex2 = ex;
									typErrStr.errStr = "Помилка обробки офлайн чека внесення/винесення готівки.";
									typErrStr.errCode = 20;
									typErrStr.ReturnStr = "";
									result = typErrStr;
									ProjectData.ClearProjectError();
									goto end_IL_003d;
								}
								typErrStr.ReturnStr = "_CheckID=" + CheckTaxNum;
								result = typErrStr;
							}
						}
					}
					else
					{
						All.MacTempOld = "";
						TypErrStr typErrStr4 = NameDotNoDot(text2, sDot: true);
						if (typErrStr4.errCode > 0)
						{
							typErrStr.errCode = typErrStr4.errCode;
							typErrStr.errStr = typErrStr4.errStr;
							result = typErrStr;
						}
						else
						{
							text2 = NameDotNoDot(text2, sDot: false).ReturnStr;
							string text4 = All.MyDoc() + "\\WebCheck\\Temp\\" + All.A.FN + "\\" + num + ".xml";
							All.SaveToFileText(text4, text2);
							TypErrOperKeyPass typErrOperKeyPass = All.l.OperatorKeyPass(NumberShift);
							if (typErrOperKeyPass.errCode > 0)
							{
								typErrStr.errCode = typErrOperKeyPass.errCode;
								typErrStr.errStr = typErrOperKeyPass.errStr;
								result = typErrStr;
							}
							else
							{
								TypErr typErr2 = All.SF.SignatureFile(typErrOperKeyPass.KeyFile.Trim(), typErrOperKeyPass.Pass.Trim(), text4);
								if (typErr2.errCode > 0)
								{
									typErrStr.errCode = typErr2.errCode;
									typErrStr.errStr = typErr2.errStr;
									typErrStr.ReturnStr = "";
									result = typErrStr;
								}
								else
								{
									string pathFile = text4;
									text4 += ".p7s";
									CheckIDv = num.ToString();
									SubmitPtr submitPtr = default(SubmitPtr);
									TypErrSubmit typErrSubmit = submitPtr.SubmitCheck(text4, CheckIDv, 3, dd);
									if (typErrSubmit.errCode > 0)
									{
										typErrStr.errCode = typErrSubmit.errCode;
										typErrStr.errStr = typErrSubmit.errStr + "  Status: " + typErrSubmit.returnStatus + "  Msg: " + typErrSubmit.returnStr + "   ";
										result = typErrStr;
									}
									else if (typErrSubmit.returnStatus < 0)
									{
										typErrStr.errCode = 26;
										typErrStr.errStr = "Служебный чек не принят сервером. Ответ  Status: " + typErrSubmit.returnStatus + "  Msg:" + typErrSubmit.returnStr + "   ";
										result = typErrStr;
									}
									else if (typErrSubmit.returnStatus == 0)
									{
										typErrStr.errCode = 32;
										typErrStr.errStr = "Перехід в режим офлайн";
										result = typErrStr;
									}
									else
									{
										CheckTaxNum = typErrSubmit.returnNumber;
										int num3 = Conversions.ToInteger(All.l.MaxID("CHECKHEAD").ReturnStr) + 1;
										typErr2 = All.l.SaveXMLcheck(num3.ToString(), typErrStr4.ReturnStr, text2, typErrSubmit.returnStr, typErrSubmit.returnNumber, text, All.Bablo(typErrStr.ReturnStr), pathFile);
										if (typErr2.errCode > 0)
										{
											typErrStr.errStr = typErr2.errStr;
											typErrStr.errCode = typErr2.errCode;
											typErrStr.ReturnStr = "";
											result = typErrStr;
										}
										else
										{
											typErrStr = All.l.SaveDealCheck(typErrStr.ReturnStr, text, parametrToString.ReturnStr, typErrSubmit.returnNumber);
											if (typErrStr.errCode <= 0)
											{
												goto IL_087d;
											}
											result = typErrStr;
										}
									}
								}
							}
						}
					}
				}
				end_IL_003d:;
			}
			catch (Exception ex3)
			{
				ProjectData.SetProjectError(ex3);
				Exception ex4 = ex3;
				typErrStr.errStr = "Помилка обробки чека внесення/винесення готівки.";
				typErrStr.errCode = 20;
				typErrStr.ReturnStr = "";
				result = typErrStr;
				ProjectData.ClearProjectError();
			}
			goto IL_0896;
		}
		IL_087d:
		typErrStr.ReturnStr = "_CheckID=" + CheckTaxNum;
		result = typErrStr;
		goto IL_0896;
		IL_0896:
		return result;
	}

	public TypErrStr FulStatusPro(string sXML)
	{
		TypErrStr typErrStr = default(TypErrStr);
		typErrStr.errCode = 0;
		typErrStr.errStr = "";
		typErrStr.ReturnStr = "";
		int num = 0;
		OperatorsAll operatorsAll = new OperatorsAll();
		string text = "<OutputParameters>";
		text += "<Parameters Err='0' ";
		text = text + "TIN='" + All.A.TIN + "' ";
		text = text + "FN='" + All.A.FN + "' ";
		string text2 = "comma";
		if (All.A.PointRegion)
		{
			text2 = "dot";
		}
		text = text + "RegionSeparator='" + text2 + "' ";
		text = text + "versionDLL='" + All.VersionDll() + "' ";
		text2 = (All.A.FullVersion ? All.A.Fullend : "free");
		text = text + "license='" + text2 + "' >";
		text += "<taxobjects>";
		text = text + "<taxobject FN='" + All.A.FN + "' ";
		text += "ID='1' ";
		text = text + "INN='" + All.A.INN + "' ";
		text = text + "ORGNAME='" + All.l.TextToTextXML(All.A.OrgName) + "' ";
		text = text + "POINTADDR='" + All.l.TextToTextXML(All.A.PointAddr) + "' ";
		text = text + "POINTNAME='" + All.l.TextToTextXML(All.A.PointName) + "' ";
		text = text + "TIN='" + All.A.TIN + "' ";
		text += "/>";
		text += "</taxobjects>";
		text += "<operators>";
		int operators = operatorsAll.Operators;
		TypErrStr result;
		checked
		{
			for (num = 1; num <= operators; num++)
			{
				text = text + "<Operator ID='" + operatorsAll.get_Seller(0, num) + "' ";
				text = text + "INN='" + operatorsAll.get_Seller(4, num) + "' ";
				text = text + "KEYPASS='" + operatorsAll.get_Seller(3, num) + "' ";
				text = text + "KEYPATH='" + operatorsAll.get_Seller(2, num) + "' ";
				text = text + "OPERATORNAME='" + operatorsAll.get_Seller(1, num) + "' ";
				text += "/>";
			}
			text += "</operators>";
			text += "<taxes>";
			int taxN = All.PayTax.TaxN;
			for (num = 1; num <= taxN; num++)
			{
				text = text + "<tax id='" + num + "' ";
				text = text + "EXCISE='" + All.PayTax.get_TaxEXCISE(num) + "' ";
				text = text + "NAME='" + All.PayTax.get_TaxName(num) + "' ";
				text = text + "TAXPRC='" + All.PayTax.get_TaxPRC(num) + "' ";
				text += "/>";
			}
			text += "</taxes>";
			text += "<payforms>";
			int payN = All.PayTax.PayN;
			for (num = 1; num <= payN; num++)
			{
				text = text + "<payform id='" + num + "' ";
				text = text + "NAME='" + All.PayTax.get_PayName(num) + "' ";
				text = text + "ISCASH='" + All.PayTax.get_PayISCASH(num) + "' ";
				text += "/>";
			}
			text += "</payforms>";
			text += "</Parameters>";
			text += "</OutputParameters>";
			try
			{
				new XmlDocument().LoadXml(text.Trim().ToLower());
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				typErrStr.ReturnStr = "";
				typErrStr.errCode = 1005;
				typErrStr.errStr = "Помилка формування XML для відповіді";
				result = typErrStr;
				ProjectData.ClearProjectError();
				goto IL_048e;
			}
			typErrStr.ReturnStr = text;
			result = typErrStr;
			goto IL_048e;
		}
		IL_048e:
		return result;
	}

	public bool VerifyXML(string strXMLv, string nameXMLfile = "")
	{
		bool result;
		try
		{
			XmlDocument xmlDocument = new XmlDocument();
			xmlDocument.LoadXml(strXMLv);
			if (nameXMLfile.Length > 0)
			{
				string filename = All.MyDoc() + "\\WebCheck\\Temp\\" + nameXMLfile + ".xml";
				xmlDocument.Save(filename);
			}
			result = true;
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

	internal string TaxDot(string taxS, bool dot = true)
	{
		string text = "^";
		taxS = ((!dot) ? Strings.Replace(taxS, ".", text) : Strings.Replace(taxS, text, "."));
		return taxS;
	}

	internal TypErrStr NameDotNoDot(string XmlS, bool sDot, bool MacOn = true)
	{
		TypErrStr result = default(TypErrStr);
		result.errCode = 0;
		result.errStr = "";
		result.ReturnStr = "";
		if (!sDot)
		{
			XmlS = Strings.Replace(XmlS, ".", "");
		}
		XmlS = Strings.Replace(XmlS, "#`#", ".");
		if (MacOn)
		{
			result = SkinXML(XmlS);
			if (result.errCode > 0)
			{
				return result;
			}
			XmlS = result.ReturnStr;
		}
		result.ReturnStr = Strings.Replace(XmlS, '\''.ToString(), '"'.ToString());
		return result;
	}

	private TypErrStr SkinXML(string xmlOld)
	{
		TypErrStr result = default(TypErrStr);
		result.errCode = 0;
		result.errStr = "";
		result.ReturnStr = "";
		TypErrStr typErrStr = All.SubstitutePreviousMAC(string.Concat("<RQ V='1'>" + xmlOld, "mmmaaaccc</RQ>"), NumberShift);
		if (typErrStr.errCode > 0)
		{
			result.errCode = typErrStr.errCode;
			result.errStr = typErrStr.errStr;
			return result;
		}
		result.ReturnStr = typErrStr.ReturnStr;
		return result;
	}

	private string RK(string sGr)
	{
		if (!All.A.RoundingCash)
		{
			return sGr;
		}
		sGr = sGr.Trim();
		int length = sGr.Length;
		if (length < 3)
		{
			return sGr;
		}
		string text = "";
		checked
		{
			int num = length - 2;
			for (int i = 0; i <= num; i++)
			{
				text += Conversions.ToString(sGr[i]);
			}
			string text2 = Conversions.ToString(sGr[length - 1]);
			if (!Versioned.IsNumeric(text2))
			{
				return "0";
			}
			int num2 = Conversions.ToInteger(text2);
			double num3 = All.StrToDouble(text);
			if (num2 > 4)
			{
				num3 += 0.1;
			}
			return num3.ToString();
		}
	}

	internal TypErrStrUpdate UPloadXML()
	{
		All.DelFileControl("C:\\ProgramData\\WebCheck\\updates\\update64.aiu");
		All.DelFileControl("C:\\ProgramData\\WebCheck\\updates\\update32.aiu");
		All.DelFileControl("C:\\ProgramData\\WebCheck\\updates\\Update\\SetupWebCheck64.msi");
		All.DelFileControl("C:\\ProgramData\\WebCheck\\updates\\Update\\SetupWebCheck32.msi");
		TypErrStrUpdate result = default(TypErrStrUpdate);
		result.errCode = 0;
		result.errStr = "";
		result.ReturnStr = "";
		result.ReturnVer = "";
		result.ReturnVerDt = "";
		result.ReturnUplink = "";
		result.ReturnImp = "";
		result.ReturnInf = "";
		result.UpdateTrue = false;
		new HttpClient().Timeout = TimeSpan.FromSeconds(5.0);
		try
		{
			XmlDocument xmlDocument = new XmlDocument();
			xmlDocument.Load("http://lic.webchek.com.ua/wchkupdatever.lic");
			result.ReturnStr = TegXml(xmlDocument.InnerXml);
			result.ReturnVer = All.d.GetParametrToString(result.ReturnStr, "ver", "InputParameters/Parameters", RegUpLow: true).ReturnStr;
			result.ReturnVerDt = All.d.GetParametrToString(result.ReturnStr, "dt", "InputParameters/Parameters", RegUpLow: true).ReturnStr;
			result.ReturnUplink = All.d.GetParametrToString(result.ReturnStr, "uplink", "InputParameters/Parameters", RegUpLow: true).ReturnStr;
			result.ReturnImp = All.d.GetParametrToString(result.ReturnStr, "importantupdate", "InputParameters/Parameters", RegUpLow: true).ReturnStr;
			result.ReturnInf = All.d.GetParametrToString(result.ReturnStr, "updateinf", "InputParameters/Parameters", RegUpLow: true).ReturnStr;
			result.ReturnStr = "6.0.8.1368";
			result.ReturnInf = result.ReturnInf.Replace("@@", "\r\n");
			if (VerInt("6.0.8.1368") < VerInt(result.ReturnVer))
			{
				result.UpdateTrue = true;
			}
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result.ReturnStr = "6.0.8.1368";
			result.errCode = 107;
			result.errStr = ex2.Message;
			ProjectData.ClearProjectError();
		}
		return result;
	}

	private int VerInt(string verStr)
	{
		verStr = verStr.Replace(".", "q");
		verStr = verStr.Replace(",", "q");
		checked
		{
			int result;
			try
			{
				verStr = verStr.Substring(verStr.Length - 5, 5);
				if (Versioned.IsNumeric(verStr))
				{
					result = Conversions.ToInteger(verStr);
				}
				else
				{
					verStr = verStr.Substring(verStr.Length - 4, 4);
					result = Conversions.ToInteger(verStr);
				}
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				result = 0;
				ProjectData.ClearProjectError();
			}
			return result;
		}
	}
}
