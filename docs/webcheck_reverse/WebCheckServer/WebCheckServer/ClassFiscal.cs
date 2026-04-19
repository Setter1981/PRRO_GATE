using Microsoft.VisualBasic.CompilerServices;
using WebCheck;
using WebCheck1;
using WebCheck10;
using WebCheck11;
using WebCheck12;
using WebCheck13;
using WebCheck14;
using WebCheck15;
using WebCheck16;
using WebCheck17;
using WebCheck18;
using WebCheck19;
using WebCheck2;
using WebCheck20;
using WebCheck21;
using WebCheck22;
using WebCheck23;
using WebCheck24;
using WebCheck25;
using WebCheck26;
using WebCheck27;
using WebCheck28;
using WebCheck29;
using WebCheck3;
using WebCheck30;
using WebCheck4;
using WebCheck5;
using WebCheck6;
using WebCheck7;
using WebCheck8;
using WebCheck9;

namespace WebCheckServer;

public class ClassFiscal
{
	public ClassFiscal()
	{
		checked
		{
			All.kuN++;
			All.NewFolder();
			All.TestRegion();
			All.A.FN = "";
			All.A.Status = false;
			All.A.ErrHelp = "";
			All.A.ErrCode = 0;
			All.A.PointRegion = false;
			All.A.FullVersion = false;
			All.A.Fullend = "";
			All.A.PathKey = "";
			All.A.PassKey = "";
			int num = 0;
			do
			{
				All.ReP[num].FN = "";
				All.ReP[num].ReplyErr = "";
				All.ReP[num].ReplyPrt = "";
				All.ReP[num].ClearControl = 0;
				num++;
			}
			while (num <= 333);
			All.W1.ServerSetGet.TypWork = 2019;
			All.W2.ServerSetGet.TypWork = 2019;
			All.W3.ServerSetGet.TypWork = 2019;
			All.W4.ServerSetGet.TypWork = 2019;
			All.W5.ServerSetGet.TypWork = 2019;
			All.W6.ServerSetGet.TypWork = 2019;
			All.W7.ServerSetGet.TypWork = 2019;
			All.W8.ServerSetGet.TypWork = 2019;
			All.W9.ServerSetGet.TypWork = 2019;
			All.W10.ServerSetGet.TypWork = 2019;
			All.W11.ServerSetGet.TypWork = 2019;
			All.W12.ServerSetGet.TypWork = 2019;
			All.W13.ServerSetGet.TypWork = 2019;
			All.W14.ServerSetGet.TypWork = 2019;
			All.W15.ServerSetGet.TypWork = 2019;
			All.W16.ServerSetGet.TypWork = 2019;
			All.W17.ServerSetGet.TypWork = 2019;
			All.W18.ServerSetGet.TypWork = 2019;
			All.W19.ServerSetGet.TypWork = 2019;
			All.W20.ServerSetGet.TypWork = 2019;
			All.W21.ServerSetGet.TypWork = 2019;
			All.W22.ServerSetGet.TypWork = 2019;
			All.W23.ServerSetGet.TypWork = 2019;
			All.W24.ServerSetGet.TypWork = 2019;
			All.W25.ServerSetGet.TypWork = 2019;
			All.W26.ServerSetGet.TypWork = 2019;
			All.W27.ServerSetGet.TypWork = 2019;
			All.W28.ServerSetGet.TypWork = 2019;
			All.W29.ServerSetGet.TypWork = 2019;
			All.W30.ServerSetGet.TypWork = 2019;
		}
	}

	~ClassFiscal()
	{
		checked
		{
			All.kuN--;
			base.Finalize();
		}
	}

	public bool Initialization(string strFN)
	{
		TypErrStr typErrStr = All.TestFN(strFN);
		if (typErrStr.errCode > 0)
		{
			All.ReplyRemember(typErrStr.FN, All.XMLRiplyErr(typErrStr.FN, typErrStr.errStr));
			All.Log.SaveTextToLog(typErrStr.FN, "Initialization", strFN, typErrStr.errStr);
			return false;
		}
		All.ReplyRemember(typErrStr.FN, All.XMLRiplyOkInitialization(typErrStr.FN));
		return true;
	}

	public bool Finalization(string strFN)
	{
		TypErrStr typErrStr = All.TestFN(strFN, "", TransactionControl: false);
		if (typErrStr.errCode > 0)
		{
			All.ReplyRemember(typErrStr.FN, All.XMLRiplyErr(typErrStr.FN, typErrStr.errStr));
			All.Log.SaveTextToLog(typErrStr.FN, "Finalization", strFN, typErrStr.errStr);
			return false;
		}
		if (Operators.CompareString(All.W1.StatusFN(), typErrStr.FN, false) == 0)
		{
			All.W1.Finalization(strFN);
		}
		if (Operators.CompareString(All.W2.StatusFN(), typErrStr.FN, false) == 0)
		{
			All.W2.Finalization(strFN);
		}
		if (Operators.CompareString(All.W3.StatusFN(), typErrStr.FN, false) == 0)
		{
			All.W3.Finalization(strFN);
		}
		if (Operators.CompareString(All.W4.StatusFN(), typErrStr.FN, false) == 0)
		{
			All.W4.Finalization(strFN);
		}
		if (Operators.CompareString(All.W5.StatusFN(), typErrStr.FN, false) == 0)
		{
			All.W5.Finalization(strFN);
		}
		if (Operators.CompareString(All.W6.StatusFN(), typErrStr.FN, false) == 0)
		{
			All.W6.Finalization(strFN);
		}
		if (Operators.CompareString(All.W7.StatusFN(), typErrStr.FN, false) == 0)
		{
			All.W7.Finalization(strFN);
		}
		if (Operators.CompareString(All.W8.StatusFN(), typErrStr.FN, false) == 0)
		{
			All.W8.Finalization(strFN);
		}
		if (Operators.CompareString(All.W9.StatusFN(), typErrStr.FN, false) == 0)
		{
			All.W9.Finalization(strFN);
		}
		if (Operators.CompareString(All.W10.StatusFN(), typErrStr.FN, false) == 0)
		{
			All.W10.Finalization(strFN);
		}
		if (Operators.CompareString(All.W11.StatusFN(), typErrStr.FN, false) == 0)
		{
			All.W11.Finalization(strFN);
		}
		if (Operators.CompareString(All.W12.StatusFN(), typErrStr.FN, false) == 0)
		{
			All.W12.Finalization(strFN);
		}
		if (Operators.CompareString(All.W13.StatusFN(), typErrStr.FN, false) == 0)
		{
			All.W13.Finalization(strFN);
		}
		if (Operators.CompareString(All.W14.StatusFN(), typErrStr.FN, false) == 0)
		{
			All.W14.Finalization(strFN);
		}
		if (Operators.CompareString(All.W15.StatusFN(), typErrStr.FN, false) == 0)
		{
			All.W15.Finalization(strFN);
		}
		if (Operators.CompareString(All.W16.StatusFN(), typErrStr.FN, false) == 0)
		{
			All.W16.Finalization(strFN);
		}
		if (Operators.CompareString(All.W17.StatusFN(), typErrStr.FN, false) == 0)
		{
			All.W17.Finalization(strFN);
		}
		if (Operators.CompareString(All.W18.StatusFN(), typErrStr.FN, false) == 0)
		{
			All.W18.Finalization(strFN);
		}
		if (Operators.CompareString(All.W19.StatusFN(), typErrStr.FN, false) == 0)
		{
			All.W19.Finalization(strFN);
		}
		if (Operators.CompareString(All.W20.StatusFN(), typErrStr.FN, false) == 0)
		{
			All.W20.Finalization(strFN);
		}
		if (Operators.CompareString(All.W21.StatusFN(), typErrStr.FN, false) == 0)
		{
			All.W21.Finalization(strFN);
		}
		if (Operators.CompareString(All.W22.StatusFN(), typErrStr.FN, false) == 0)
		{
			All.W21.Finalization(strFN);
		}
		if (Operators.CompareString(All.W23.StatusFN(), typErrStr.FN, false) == 0)
		{
			All.W21.Finalization(strFN);
		}
		if (Operators.CompareString(All.W24.StatusFN(), typErrStr.FN, false) == 0)
		{
			All.W21.Finalization(strFN);
		}
		if (Operators.CompareString(All.W25.StatusFN(), typErrStr.FN, false) == 0)
		{
			All.W21.Finalization(strFN);
		}
		if (Operators.CompareString(All.W26.StatusFN(), typErrStr.FN, false) == 0)
		{
			All.W21.Finalization(strFN);
		}
		if (Operators.CompareString(All.W27.StatusFN(), typErrStr.FN, false) == 0)
		{
			All.W21.Finalization(strFN);
		}
		if (Operators.CompareString(All.W28.StatusFN(), typErrStr.FN, false) == 0)
		{
			All.W21.Finalization(strFN);
		}
		if (Operators.CompareString(All.W29.StatusFN(), typErrStr.FN, false) == 0)
		{
			All.W21.Finalization(strFN);
		}
		if (Operators.CompareString(All.W30.StatusFN(), typErrStr.FN, false) == 0)
		{
			All.W21.Finalization(strFN);
		}
		All.ReplyRemember(typErrStr.FN, All.XMLRiplyOk(typErrStr.FN));
		return true;
	}

	public bool OpenShift(string strFN)
	{
		checked
		{
			All.sS++;
			TypErrStr typErrStr = All.TestFN(strFN);
			if (typErrStr.errCode > 0)
			{
				All.ReplyRemember(typErrStr.FN, All.XMLRiplyErr(typErrStr.FN, typErrStr.errStr));
				All.Log.SaveTextToLog(typErrStr.FN, "OpenShift", strFN, typErrStr.errStr);
				return false;
			}
			string strFN2 = "<InputParameters><Parameters FN='" + typErrStr.FN + "'/></InputParameters>";
			WebCheck1.ClassFiscal w = All.W1;
			if (Operators.CompareString(w.StatusFN(), "", false) == 0 && w.Initialization(strFN2))
			{
				bool num = w.OpenShift(strFN);
				string repXML = "";
				if (num)
				{
					repXML = w.CheckLine.CheckXML("");
				}
				All.ReplyRemember(typErrStr.FN, w.StatusBarXML(), repXML);
				w.Finalization(strFN2);
				return num;
			}
			w = null;
			WebCheck2.ClassFiscal w2 = All.W2;
			if (Operators.CompareString(w2.StatusFN(), "", false) == 0 && w2.Initialization(strFN2))
			{
				bool num2 = w2.OpenShift(strFN);
				string repXML2 = "";
				if (num2)
				{
					repXML2 = w2.CheckLine.CheckXML("");
				}
				All.ReplyRemember(typErrStr.FN, w2.StatusBarXML(), repXML2);
				w2.Finalization(strFN2);
				return num2;
			}
			w2 = null;
			WebCheck3.ClassFiscal w3 = All.W3;
			if (Operators.CompareString(w3.StatusFN(), "", false) == 0 && w3.Initialization(strFN2))
			{
				bool num3 = w3.OpenShift(strFN);
				string repXML3 = "";
				if (num3)
				{
					repXML3 = w3.CheckLine.CheckXML("");
				}
				All.ReplyRemember(typErrStr.FN, w3.StatusBarXML(), repXML3);
				w3.Finalization(strFN2);
				return num3;
			}
			w3 = null;
			WebCheck4.ClassFiscal w4 = All.W4;
			if (Operators.CompareString(w4.StatusFN(), "", false) == 0 && w4.Initialization(strFN2))
			{
				bool num4 = w4.OpenShift(strFN);
				string repXML4 = "";
				if (num4)
				{
					repXML4 = w4.CheckLine.CheckXML("");
				}
				All.ReplyRemember(typErrStr.FN, w4.StatusBarXML(), repXML4);
				w4.Finalization(strFN2);
				return num4;
			}
			w4 = null;
			WebCheck5.ClassFiscal w5 = All.W5;
			if (Operators.CompareString(w5.StatusFN(), "", false) == 0 && w5.Initialization(strFN2))
			{
				bool num5 = w5.OpenShift(strFN);
				string repXML5 = "";
				if (num5)
				{
					repXML5 = w5.CheckLine.CheckXML("");
				}
				All.ReplyRemember(typErrStr.FN, w5.StatusBarXML(), repXML5);
				w5.Finalization(strFN2);
				return num5;
			}
			w5 = null;
			WebCheck6.ClassFiscal w6 = All.W6;
			if (Operators.CompareString(w6.StatusFN(), "", false) == 0 && w6.Initialization(strFN2))
			{
				bool num6 = w6.OpenShift(strFN);
				string repXML6 = "";
				if (num6)
				{
					repXML6 = w6.CheckLine.CheckXML("");
				}
				All.ReplyRemember(typErrStr.FN, w6.StatusBarXML(), repXML6);
				w6.Finalization(strFN2);
				return num6;
			}
			w6 = null;
			WebCheck7.ClassFiscal w7 = All.W7;
			if (Operators.CompareString(w7.StatusFN(), "", false) == 0 && w7.Initialization(strFN2))
			{
				bool num7 = w7.OpenShift(strFN);
				string repXML7 = "";
				if (num7)
				{
					repXML7 = w7.CheckLine.CheckXML("");
				}
				All.ReplyRemember(typErrStr.FN, w7.StatusBarXML(), repXML7);
				w7.Finalization(strFN2);
				return num7;
			}
			w7 = null;
			WebCheck8.ClassFiscal w8 = All.W8;
			if (Operators.CompareString(w8.StatusFN(), "", false) == 0 && w8.Initialization(strFN2))
			{
				bool num8 = w8.OpenShift(strFN);
				string repXML8 = "";
				if (num8)
				{
					repXML8 = w8.CheckLine.CheckXML("");
				}
				All.ReplyRemember(typErrStr.FN, w8.StatusBarXML(), repXML8);
				w8.Finalization(strFN2);
				return num8;
			}
			w8 = null;
			WebCheck9.ClassFiscal w9 = All.W9;
			if (Operators.CompareString(w9.StatusFN(), "", false) == 0 && w9.Initialization(strFN2))
			{
				bool num9 = w9.OpenShift(strFN);
				string repXML9 = "";
				if (num9)
				{
					repXML9 = w9.CheckLine.CheckXML("");
				}
				All.ReplyRemember(typErrStr.FN, w9.StatusBarXML(), repXML9);
				w9.Finalization(strFN2);
				return num9;
			}
			w9 = null;
			WebCheck10.ClassFiscal w10 = All.W10;
			if (Operators.CompareString(w10.StatusFN(), "", false) == 0 && w10.Initialization(strFN2))
			{
				bool num10 = w10.OpenShift(strFN);
				string repXML10 = "";
				if (num10)
				{
					repXML10 = w10.CheckLine.CheckXML("");
				}
				All.ReplyRemember(typErrStr.FN, w10.StatusBarXML(), repXML10);
				w10.Finalization(strFN2);
				return num10;
			}
			w10 = null;
			WebCheck11.ClassFiscal w11 = All.W11;
			if (Operators.CompareString(w11.StatusFN(), "", false) == 0 && w11.Initialization(strFN2))
			{
				bool num11 = w11.OpenShift(strFN);
				string repXML11 = "";
				if (num11)
				{
					repXML11 = w11.CheckLine.CheckXML("");
				}
				All.ReplyRemember(typErrStr.FN, w11.StatusBarXML(), repXML11);
				w11.Finalization(strFN2);
				return num11;
			}
			w11 = null;
			WebCheck12.ClassFiscal w12 = All.W12;
			if (Operators.CompareString(w12.StatusFN(), "", false) == 0 && w12.Initialization(strFN2))
			{
				bool num12 = w12.OpenShift(strFN);
				string repXML12 = "";
				if (num12)
				{
					repXML12 = w12.CheckLine.CheckXML("");
				}
				All.ReplyRemember(typErrStr.FN, w12.StatusBarXML(), repXML12);
				w12.Finalization(strFN2);
				return num12;
			}
			w12 = null;
			WebCheck13.ClassFiscal w13 = All.W13;
			if (Operators.CompareString(w13.StatusFN(), "", false) == 0 && w13.Initialization(strFN2))
			{
				bool num13 = w13.OpenShift(strFN);
				string repXML13 = "";
				if (num13)
				{
					repXML13 = w13.CheckLine.CheckXML("");
				}
				All.ReplyRemember(typErrStr.FN, w13.StatusBarXML(), repXML13);
				w13.Finalization(strFN2);
				return num13;
			}
			w13 = null;
			WebCheck14.ClassFiscal w14 = All.W14;
			if (Operators.CompareString(w14.StatusFN(), "", false) == 0 && w14.Initialization(strFN2))
			{
				bool num14 = w14.OpenShift(strFN);
				string repXML14 = "";
				if (num14)
				{
					repXML14 = w14.CheckLine.CheckXML("");
				}
				All.ReplyRemember(typErrStr.FN, w14.StatusBarXML(), repXML14);
				w14.Finalization(strFN2);
				return num14;
			}
			w14 = null;
			WebCheck15.ClassFiscal w15 = All.W15;
			if (Operators.CompareString(w15.StatusFN(), "", false) == 0 && w15.Initialization(strFN2))
			{
				bool num15 = w15.OpenShift(strFN);
				string repXML15 = "";
				if (num15)
				{
					repXML15 = w15.CheckLine.CheckXML("");
				}
				All.ReplyRemember(typErrStr.FN, w15.StatusBarXML(), repXML15);
				w15.Finalization(strFN2);
				return num15;
			}
			w15 = null;
			WebCheck16.ClassFiscal w16 = All.W16;
			if (Operators.CompareString(w16.StatusFN(), "", false) == 0 && w16.Initialization(strFN2))
			{
				bool num16 = w16.OpenShift(strFN);
				string repXML16 = "";
				if (num16)
				{
					repXML16 = w16.CheckLine.CheckXML("");
				}
				All.ReplyRemember(typErrStr.FN, w16.StatusBarXML(), repXML16);
				w16.Finalization(strFN2);
				return num16;
			}
			w16 = null;
			WebCheck17.ClassFiscal w17 = All.W17;
			if (Operators.CompareString(w17.StatusFN(), "", false) == 0 && w17.Initialization(strFN2))
			{
				bool num17 = w17.OpenShift(strFN);
				string repXML17 = "";
				if (num17)
				{
					repXML17 = w17.CheckLine.CheckXML("");
				}
				All.ReplyRemember(typErrStr.FN, w17.StatusBarXML(), repXML17);
				w17.Finalization(strFN2);
				return num17;
			}
			w17 = null;
			WebCheck18.ClassFiscal w18 = All.W18;
			if (Operators.CompareString(w18.StatusFN(), "", false) == 0 && w18.Initialization(strFN2))
			{
				bool num18 = w18.OpenShift(strFN);
				string repXML18 = "";
				if (num18)
				{
					repXML18 = w18.CheckLine.CheckXML("");
				}
				All.ReplyRemember(typErrStr.FN, w18.StatusBarXML(), repXML18);
				w18.Finalization(strFN2);
				return num18;
			}
			w18 = null;
			WebCheck19.ClassFiscal w19 = All.W19;
			if (Operators.CompareString(w19.StatusFN(), "", false) == 0 && w19.Initialization(strFN2))
			{
				bool num19 = w19.OpenShift(strFN);
				string repXML19 = "";
				if (num19)
				{
					repXML19 = w19.CheckLine.CheckXML("");
				}
				All.ReplyRemember(typErrStr.FN, w19.StatusBarXML(), repXML19);
				w19.Finalization(strFN2);
				return num19;
			}
			w19 = null;
			WebCheck20.ClassFiscal w20 = All.W20;
			if (Operators.CompareString(w20.StatusFN(), "", false) == 0 && w20.Initialization(strFN2))
			{
				bool num20 = w20.OpenShift(strFN);
				string repXML20 = "";
				if (num20)
				{
					repXML20 = w20.CheckLine.CheckXML("");
				}
				All.ReplyRemember(typErrStr.FN, w20.StatusBarXML(), repXML20);
				w20.Finalization(strFN2);
				return num20;
			}
			w20 = null;
			WebCheck21.ClassFiscal w21 = All.W21;
			if (Operators.CompareString(w21.StatusFN(), "", false) == 0 && w21.Initialization(strFN2))
			{
				bool num21 = w21.OpenShift(strFN);
				string repXML21 = "";
				if (num21)
				{
					repXML21 = w21.CheckLine.CheckXML("");
				}
				All.ReplyRemember(typErrStr.FN, w21.StatusBarXML(), repXML21);
				w21.Finalization(strFN2);
				return num21;
			}
			w21 = null;
			WebCheck22.ClassFiscal w22 = All.W22;
			if (Operators.CompareString(w22.StatusFN(), "", false) == 0 && w22.Initialization(strFN2))
			{
				bool num22 = w22.OpenShift(strFN);
				string repXML22 = "";
				if (num22)
				{
					repXML22 = w22.CheckLine.CheckXML("");
				}
				All.ReplyRemember(typErrStr.FN, w22.StatusBarXML(), repXML22);
				w22.Finalization(strFN2);
				return num22;
			}
			w22 = null;
			WebCheck23.ClassFiscal w23 = All.W23;
			if (Operators.CompareString(w23.StatusFN(), "", false) == 0 && w23.Initialization(strFN2))
			{
				bool num23 = w23.OpenShift(strFN);
				string repXML23 = "";
				if (num23)
				{
					repXML23 = w23.CheckLine.CheckXML("");
				}
				All.ReplyRemember(typErrStr.FN, w23.StatusBarXML(), repXML23);
				w23.Finalization(strFN2);
				return num23;
			}
			w23 = null;
			WebCheck24.ClassFiscal w24 = All.W24;
			if (Operators.CompareString(w24.StatusFN(), "", false) == 0 && w24.Initialization(strFN2))
			{
				bool num24 = w24.OpenShift(strFN);
				string repXML24 = "";
				if (num24)
				{
					repXML24 = w24.CheckLine.CheckXML("");
				}
				All.ReplyRemember(typErrStr.FN, w24.StatusBarXML(), repXML24);
				w24.Finalization(strFN2);
				return num24;
			}
			w24 = null;
			WebCheck25.ClassFiscal w25 = All.W25;
			if (Operators.CompareString(w25.StatusFN(), "", false) == 0 && w25.Initialization(strFN2))
			{
				bool num25 = w25.OpenShift(strFN);
				string repXML25 = "";
				if (num25)
				{
					repXML25 = w25.CheckLine.CheckXML("");
				}
				All.ReplyRemember(typErrStr.FN, w25.StatusBarXML(), repXML25);
				w25.Finalization(strFN2);
				return num25;
			}
			w25 = null;
			WebCheck26.ClassFiscal w26 = All.W26;
			if (Operators.CompareString(w26.StatusFN(), "", false) == 0 && w26.Initialization(strFN2))
			{
				bool num26 = w26.OpenShift(strFN);
				string repXML26 = "";
				if (num26)
				{
					repXML26 = w26.CheckLine.CheckXML("");
				}
				All.ReplyRemember(typErrStr.FN, w26.StatusBarXML(), repXML26);
				w26.Finalization(strFN2);
				return num26;
			}
			w26 = null;
			WebCheck27.ClassFiscal w27 = All.W27;
			if (Operators.CompareString(w27.StatusFN(), "", false) == 0 && w27.Initialization(strFN2))
			{
				bool num27 = w27.OpenShift(strFN);
				string repXML27 = "";
				if (num27)
				{
					repXML27 = w27.CheckLine.CheckXML("");
				}
				All.ReplyRemember(typErrStr.FN, w27.StatusBarXML(), repXML27);
				w27.Finalization(strFN2);
				return num27;
			}
			w27 = null;
			WebCheck28.ClassFiscal w28 = All.W28;
			if (Operators.CompareString(w28.StatusFN(), "", false) == 0 && w28.Initialization(strFN2))
			{
				bool num28 = w28.OpenShift(strFN);
				string repXML28 = "";
				if (num28)
				{
					repXML28 = w28.CheckLine.CheckXML("");
				}
				All.ReplyRemember(typErrStr.FN, w28.StatusBarXML(), repXML28);
				w28.Finalization(strFN2);
				return num28;
			}
			w28 = null;
			WebCheck29.ClassFiscal w29 = All.W29;
			if (Operators.CompareString(w29.StatusFN(), "", false) == 0 && w29.Initialization(strFN2))
			{
				bool num29 = w29.OpenShift(strFN);
				string repXML29 = "";
				if (num29)
				{
					repXML29 = w29.CheckLine.CheckXML("");
				}
				All.ReplyRemember(typErrStr.FN, w29.StatusBarXML(), repXML29);
				w29.Finalization(strFN2);
				return num29;
			}
			w29 = null;
			WebCheck30.ClassFiscal w30 = All.W30;
			if (Operators.CompareString(w30.StatusFN(), "", false) == 0 && w30.Initialization(strFN2))
			{
				bool num30 = w30.OpenShift(strFN);
				string repXML30 = "";
				if (num30)
				{
					repXML30 = w30.CheckLine.CheckXML("");
				}
				All.ReplyRemember(typErrStr.FN, w30.StatusBarXML(), repXML30);
				w30.Finalization(strFN2);
				return num30;
			}
			w30 = null;
			All.sF++;
			string text = All.sS + "/" + All.sF;
			All.Log.SaveTextToLog(typErrStr.FN, "OpenShift " + text, strFN, "Все слоты заняты транзакциями с сервером налоговой");
			All.ReplyRemember(typErrStr.FN, All.XMLRiplyErr(typErrStr.FN, "Все слоты заняты транзакциями с сервером налоговой"));
			return false;
		}
	}

	public bool FiscalReceipt(string strFN)
	{
		TypErrStr typErrStr = default(TypErrStr);
		typErrStr.FN = "";
		typErrStr.errCode = 0;
		typErrStr.errStr = "";
		if (Operators.CompareString(All.A.FN.Trim(), "", false) == 0)
		{
			typErrStr = All.TestFN(strFN, "check");
			if (typErrStr.errCode > 0)
			{
				All.ReplyRemember(typErrStr.FN, All.XMLRiplyErr(typErrStr.FN, typErrStr.errStr));
				All.Log.SaveTextToLog(typErrStr.FN, "FiscalReceipt", strFN, typErrStr.errStr);
				return false;
			}
		}
		else
		{
			string text = "<InputParameters><Parameters FN='" + All.A.FN + "'/></InputParameters>";
			All.A.FN = "";
			typErrStr = All.TestFN(text);
			if (typErrStr.errCode > 0)
			{
				All.ReplyRemember(typErrStr.FN, All.XMLRiplyErr(typErrStr.FN, typErrStr.errStr));
				All.Log.SaveTextToLog(typErrStr.FN, "FiscalReceipt", text, typErrStr.errStr);
				return false;
			}
		}
		string strFN2 = "<InputParameters><Parameters FN='" + typErrStr.FN + "'/></InputParameters>";
		WebCheck1.ClassFiscal w = All.W1;
		if (Operators.CompareString(w.StatusFN(), "", false) == 0 && w.Initialization(strFN2))
		{
			strFN = w.ServerSetGet.VkToComXML(strFN);
			bool num = w.FiscalReceipt(strFN);
			string repXML = "";
			if (num)
			{
				repXML = w.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w.StatusBarXML(), repXML);
			w.Finalization(strFN2);
			return num;
		}
		w = null;
		WebCheck2.ClassFiscal w2 = All.W2;
		if (Operators.CompareString(w2.StatusFN(), "", false) == 0 && w2.Initialization(strFN2))
		{
			strFN = w2.ServerSetGet.VkToComXML(strFN);
			bool num2 = w2.FiscalReceipt(strFN);
			string repXML2 = "";
			if (num2)
			{
				repXML2 = w2.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w2.StatusBarXML(), repXML2);
			w2.Finalization(strFN2);
			return num2;
		}
		w2 = null;
		WebCheck3.ClassFiscal w3 = All.W3;
		if (Operators.CompareString(w3.StatusFN(), "", false) == 0 && w3.Initialization(strFN2))
		{
			strFN = w3.ServerSetGet.VkToComXML(strFN);
			bool num3 = w3.FiscalReceipt(strFN);
			string repXML3 = "";
			if (num3)
			{
				repXML3 = w3.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w3.StatusBarXML(), repXML3);
			w3.Finalization(strFN2);
			return num3;
		}
		w3 = null;
		WebCheck4.ClassFiscal w4 = All.W4;
		if (Operators.CompareString(w4.StatusFN(), "", false) == 0 && w4.Initialization(strFN2))
		{
			strFN = w4.ServerSetGet.VkToComXML(strFN);
			bool num4 = w4.FiscalReceipt(strFN);
			string repXML4 = "";
			if (num4)
			{
				repXML4 = w4.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w4.StatusBarXML(), repXML4);
			w4.Finalization(strFN2);
			return num4;
		}
		w4 = null;
		WebCheck5.ClassFiscal w5 = All.W5;
		if (Operators.CompareString(w5.StatusFN(), "", false) == 0 && w5.Initialization(strFN2))
		{
			strFN = w5.ServerSetGet.VkToComXML(strFN);
			bool num5 = w5.FiscalReceipt(strFN);
			string repXML5 = "";
			if (num5)
			{
				repXML5 = w5.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w5.StatusBarXML(), repXML5);
			w5.Finalization(strFN2);
			return num5;
		}
		w5 = null;
		WebCheck6.ClassFiscal w6 = All.W6;
		if (Operators.CompareString(w6.StatusFN(), "", false) == 0 && w6.Initialization(strFN2))
		{
			strFN = w6.ServerSetGet.VkToComXML(strFN);
			bool num6 = w6.FiscalReceipt(strFN);
			string repXML6 = "";
			if (num6)
			{
				repXML6 = w6.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w6.StatusBarXML(), repXML6);
			w6.Finalization(strFN2);
			return num6;
		}
		w6 = null;
		WebCheck7.ClassFiscal w7 = All.W7;
		if (Operators.CompareString(w7.StatusFN(), "", false) == 0 && w7.Initialization(strFN2))
		{
			strFN = w7.ServerSetGet.VkToComXML(strFN);
			bool num7 = w7.FiscalReceipt(strFN);
			string repXML7 = "";
			if (num7)
			{
				repXML7 = w7.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w7.StatusBarXML(), repXML7);
			w7.Finalization(strFN2);
			return num7;
		}
		w7 = null;
		WebCheck8.ClassFiscal w8 = All.W8;
		if (Operators.CompareString(w8.StatusFN(), "", false) == 0 && w8.Initialization(strFN2))
		{
			strFN = w8.ServerSetGet.VkToComXML(strFN);
			bool num8 = w8.FiscalReceipt(strFN);
			string repXML8 = "";
			if (num8)
			{
				repXML8 = w8.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w8.StatusBarXML(), repXML8);
			w8.Finalization(strFN2);
			return num8;
		}
		w8 = null;
		WebCheck9.ClassFiscal w9 = All.W9;
		if (Operators.CompareString(w9.StatusFN(), "", false) == 0 && w9.Initialization(strFN2))
		{
			strFN = w9.ServerSetGet.VkToComXML(strFN);
			bool num9 = w9.FiscalReceipt(strFN);
			string repXML9 = "";
			if (num9)
			{
				repXML9 = w9.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w9.StatusBarXML(), repXML9);
			w9.Finalization(strFN2);
			return num9;
		}
		w9 = null;
		WebCheck10.ClassFiscal w10 = All.W10;
		if (Operators.CompareString(w10.StatusFN(), "", false) == 0 && w10.Initialization(strFN2))
		{
			strFN = w10.ServerSetGet.VkToComXML(strFN);
			bool num10 = w10.FiscalReceipt(strFN);
			string repXML10 = "";
			if (num10)
			{
				repXML10 = w10.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w10.StatusBarXML(), repXML10);
			w10.Finalization(strFN2);
			return num10;
		}
		w10 = null;
		WebCheck11.ClassFiscal w11 = All.W11;
		if (Operators.CompareString(w11.StatusFN(), "", false) == 0 && w11.Initialization(strFN2))
		{
			strFN = w11.ServerSetGet.VkToComXML(strFN);
			bool num11 = w11.FiscalReceipt(strFN);
			string repXML11 = "";
			if (num11)
			{
				repXML11 = w11.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w11.StatusBarXML(), repXML11);
			w11.Finalization(strFN2);
			return num11;
		}
		w11 = null;
		WebCheck12.ClassFiscal w12 = All.W12;
		if (Operators.CompareString(w12.StatusFN(), "", false) == 0 && w12.Initialization(strFN2))
		{
			strFN = w12.ServerSetGet.VkToComXML(strFN);
			bool num12 = w12.FiscalReceipt(strFN);
			string repXML12 = "";
			if (num12)
			{
				repXML12 = w12.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w12.StatusBarXML(), repXML12);
			w12.Finalization(strFN2);
			return num12;
		}
		w12 = null;
		WebCheck13.ClassFiscal w13 = All.W13;
		if (Operators.CompareString(w13.StatusFN(), "", false) == 0 && w13.Initialization(strFN2))
		{
			strFN = w13.ServerSetGet.VkToComXML(strFN);
			bool num13 = w13.FiscalReceipt(strFN);
			string repXML13 = "";
			if (num13)
			{
				repXML13 = w13.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w13.StatusBarXML(), repXML13);
			w13.Finalization(strFN2);
			return num13;
		}
		w13 = null;
		WebCheck14.ClassFiscal w14 = All.W14;
		if (Operators.CompareString(w14.StatusFN(), "", false) == 0 && w14.Initialization(strFN2))
		{
			strFN = w14.ServerSetGet.VkToComXML(strFN);
			bool num14 = w14.FiscalReceipt(strFN);
			string repXML14 = "";
			if (num14)
			{
				repXML14 = w14.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w14.StatusBarXML(), repXML14);
			w14.Finalization(strFN2);
			return num14;
		}
		w14 = null;
		WebCheck15.ClassFiscal w15 = All.W15;
		if (Operators.CompareString(w15.StatusFN(), "", false) == 0 && w15.Initialization(strFN2))
		{
			strFN = w15.ServerSetGet.VkToComXML(strFN);
			bool num15 = w15.FiscalReceipt(strFN);
			string repXML15 = "";
			if (num15)
			{
				repXML15 = w15.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w15.StatusBarXML(), repXML15);
			w15.Finalization(strFN2);
			return num15;
		}
		w15 = null;
		WebCheck16.ClassFiscal w16 = All.W16;
		if (Operators.CompareString(w16.StatusFN(), "", false) == 0 && w16.Initialization(strFN2))
		{
			strFN = w16.ServerSetGet.VkToComXML(strFN);
			bool num16 = w16.FiscalReceipt(strFN);
			string repXML16 = "";
			if (num16)
			{
				repXML16 = w16.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w16.StatusBarXML(), repXML16);
			w16.Finalization(strFN2);
			return num16;
		}
		w16 = null;
		WebCheck17.ClassFiscal w17 = All.W17;
		if (Operators.CompareString(w17.StatusFN(), "", false) == 0 && w17.Initialization(strFN2))
		{
			strFN = w17.ServerSetGet.VkToComXML(strFN);
			bool num17 = w17.FiscalReceipt(strFN);
			string repXML17 = "";
			if (num17)
			{
				repXML17 = w17.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w17.StatusBarXML(), repXML17);
			w17.Finalization(strFN2);
			return num17;
		}
		w17 = null;
		WebCheck18.ClassFiscal w18 = All.W18;
		if (Operators.CompareString(w18.StatusFN(), "", false) == 0 && w18.Initialization(strFN2))
		{
			strFN = w18.ServerSetGet.VkToComXML(strFN);
			bool num18 = w18.FiscalReceipt(strFN);
			string repXML18 = "";
			if (num18)
			{
				repXML18 = w18.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w18.StatusBarXML(), repXML18);
			w18.Finalization(strFN2);
			return num18;
		}
		w18 = null;
		WebCheck19.ClassFiscal w19 = All.W19;
		if (Operators.CompareString(w19.StatusFN(), "", false) == 0 && w19.Initialization(strFN2))
		{
			strFN = w19.ServerSetGet.VkToComXML(strFN);
			bool num19 = w19.FiscalReceipt(strFN);
			string repXML19 = "";
			if (num19)
			{
				repXML19 = w19.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w19.StatusBarXML(), repXML19);
			w19.Finalization(strFN2);
			return num19;
		}
		w19 = null;
		WebCheck20.ClassFiscal w20 = All.W20;
		if (Operators.CompareString(w20.StatusFN(), "", false) == 0 && w20.Initialization(strFN2))
		{
			strFN = w20.ServerSetGet.VkToComXML(strFN);
			bool num20 = w20.FiscalReceipt(strFN);
			string repXML20 = "";
			if (num20)
			{
				repXML20 = w20.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w20.StatusBarXML(), repXML20);
			w20.Finalization(strFN2);
			return num20;
		}
		w20 = null;
		WebCheck21.ClassFiscal w21 = All.W21;
		if (Operators.CompareString(w21.StatusFN(), "", false) == 0 && w21.Initialization(strFN2))
		{
			strFN = w21.ServerSetGet.VkToComXML(strFN);
			bool num21 = w21.FiscalReceipt(strFN);
			string repXML21 = "";
			if (num21)
			{
				repXML21 = w21.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w21.StatusBarXML(), repXML21);
			w21.Finalization(strFN2);
			return num21;
		}
		w21 = null;
		WebCheck22.ClassFiscal w22 = All.W22;
		if (Operators.CompareString(w22.StatusFN(), "", false) == 0 && w22.Initialization(strFN2))
		{
			strFN = w22.ServerSetGet.VkToComXML(strFN);
			bool num22 = w22.FiscalReceipt(strFN);
			string repXML22 = "";
			if (num22)
			{
				repXML22 = w22.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w22.StatusBarXML(), repXML22);
			w22.Finalization(strFN2);
			return num22;
		}
		w22 = null;
		WebCheck23.ClassFiscal w23 = All.W23;
		if (Operators.CompareString(w23.StatusFN(), "", false) == 0 && w23.Initialization(strFN2))
		{
			strFN = w23.ServerSetGet.VkToComXML(strFN);
			bool num23 = w23.FiscalReceipt(strFN);
			string repXML23 = "";
			if (num23)
			{
				repXML23 = w23.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w23.StatusBarXML(), repXML23);
			w23.Finalization(strFN2);
			return num23;
		}
		w23 = null;
		WebCheck24.ClassFiscal w24 = All.W24;
		if (Operators.CompareString(w24.StatusFN(), "", false) == 0 && w24.Initialization(strFN2))
		{
			strFN = w24.ServerSetGet.VkToComXML(strFN);
			bool num24 = w24.FiscalReceipt(strFN);
			string repXML24 = "";
			if (num24)
			{
				repXML24 = w24.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w24.StatusBarXML(), repXML24);
			w24.Finalization(strFN2);
			return num24;
		}
		w24 = null;
		WebCheck25.ClassFiscal w25 = All.W25;
		if (Operators.CompareString(w25.StatusFN(), "", false) == 0 && w25.Initialization(strFN2))
		{
			strFN = w25.ServerSetGet.VkToComXML(strFN);
			bool num25 = w25.FiscalReceipt(strFN);
			string repXML25 = "";
			if (num25)
			{
				repXML25 = w25.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w25.StatusBarXML(), repXML25);
			w25.Finalization(strFN2);
			return num25;
		}
		w25 = null;
		WebCheck26.ClassFiscal w26 = All.W26;
		if (Operators.CompareString(w26.StatusFN(), "", false) == 0 && w26.Initialization(strFN2))
		{
			strFN = w26.ServerSetGet.VkToComXML(strFN);
			bool num26 = w26.FiscalReceipt(strFN);
			string repXML26 = "";
			if (num26)
			{
				repXML26 = w26.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w26.StatusBarXML(), repXML26);
			w26.Finalization(strFN2);
			return num26;
		}
		w26 = null;
		WebCheck27.ClassFiscal w27 = All.W27;
		if (Operators.CompareString(w27.StatusFN(), "", false) == 0 && w27.Initialization(strFN2))
		{
			strFN = w27.ServerSetGet.VkToComXML(strFN);
			bool num27 = w27.FiscalReceipt(strFN);
			string repXML27 = "";
			if (num27)
			{
				repXML27 = w27.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w27.StatusBarXML(), repXML27);
			w27.Finalization(strFN2);
			return num27;
		}
		w27 = null;
		All.Log.SaveTextToLog(typErrStr.FN, "FiscalReceipt", strFN, "Все слоты заняты транзакциями с сервером налоговой");
		All.ReplyRemember(typErrStr.FN, All.XMLRiplyErr(typErrStr.FN, "Все слоты заняты транзакциями с сервером налоговой"));
		return false;
	}

	public bool EPZtoCash(string strFN)
	{
		TypErrStr typErrStr = default(TypErrStr);
		typErrStr.FN = "";
		typErrStr.errCode = 0;
		typErrStr.errStr = "";
		typErrStr = All.TestFN(strFN);
		if (typErrStr.errCode > 0)
		{
			All.ReplyRemember(typErrStr.FN, All.XMLRiplyErr(typErrStr.FN, typErrStr.errStr));
			All.Log.SaveTextToLog(typErrStr.FN, "EPZtoCash", strFN, typErrStr.errStr);
			return false;
		}
		string strFN2 = "<InputParameters><Parameters FN='" + typErrStr.FN + "'/></InputParameters>";
		WebCheck1.ClassFiscal w = All.W1;
		if (Operators.CompareString(w.StatusFN(), "", false) == 0 && w.Initialization(strFN2))
		{
			bool num = w.EPZtoCash(strFN);
			string repXML = "";
			if (num)
			{
				repXML = w.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w.StatusBarXML(), repXML);
			w.Finalization(strFN2);
			return num;
		}
		w = null;
		WebCheck2.ClassFiscal w2 = All.W2;
		if (Operators.CompareString(w2.StatusFN(), "", false) == 0 && w2.Initialization(strFN2))
		{
			bool num2 = w2.EPZtoCash(strFN);
			string repXML2 = "";
			if (num2)
			{
				repXML2 = w2.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w2.StatusBarXML(), repXML2);
			w2.Finalization(strFN2);
			return num2;
		}
		w2 = null;
		WebCheck3.ClassFiscal w3 = All.W3;
		if (Operators.CompareString(w3.StatusFN(), "", false) == 0 && w3.Initialization(strFN2))
		{
			bool num3 = w3.EPZtoCash(strFN);
			string repXML3 = "";
			if (num3)
			{
				repXML3 = w3.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w3.StatusBarXML(), repXML3);
			w3.Finalization(strFN2);
			return num3;
		}
		w3 = null;
		WebCheck4.ClassFiscal w4 = All.W4;
		if (Operators.CompareString(w4.StatusFN(), "", false) == 0 && w4.Initialization(strFN2))
		{
			bool num4 = w4.EPZtoCash(strFN);
			string repXML4 = "";
			if (num4)
			{
				repXML4 = w4.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w4.StatusBarXML(), repXML4);
			w4.Finalization(strFN2);
			return num4;
		}
		w4 = null;
		WebCheck5.ClassFiscal w5 = All.W5;
		if (Operators.CompareString(w5.StatusFN(), "", false) == 0 && w5.Initialization(strFN2))
		{
			bool num5 = w5.EPZtoCash(strFN);
			string repXML5 = "";
			if (num5)
			{
				repXML5 = w5.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w5.StatusBarXML(), repXML5);
			w5.Finalization(strFN2);
			return num5;
		}
		w5 = null;
		WebCheck6.ClassFiscal w6 = All.W6;
		if (Operators.CompareString(w6.StatusFN(), "", false) == 0 && w6.Initialization(strFN2))
		{
			bool num6 = w6.EPZtoCash(strFN);
			string repXML6 = "";
			if (num6)
			{
				repXML6 = w6.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w6.StatusBarXML(), repXML6);
			w6.Finalization(strFN2);
			return num6;
		}
		w6 = null;
		WebCheck7.ClassFiscal w7 = All.W7;
		if (Operators.CompareString(w7.StatusFN(), "", false) == 0 && w7.Initialization(strFN2))
		{
			bool num7 = w7.EPZtoCash(strFN);
			string repXML7 = "";
			if (num7)
			{
				repXML7 = w7.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w7.StatusBarXML(), repXML7);
			w7.Finalization(strFN2);
			return num7;
		}
		w7 = null;
		WebCheck8.ClassFiscal w8 = All.W8;
		if (Operators.CompareString(w8.StatusFN(), "", false) == 0 && w8.Initialization(strFN2))
		{
			bool num8 = w8.EPZtoCash(strFN);
			string repXML8 = "";
			if (num8)
			{
				repXML8 = w8.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w8.StatusBarXML(), repXML8);
			w8.Finalization(strFN2);
			return num8;
		}
		w8 = null;
		WebCheck9.ClassFiscal w9 = All.W9;
		if (Operators.CompareString(w9.StatusFN(), "", false) == 0 && w9.Initialization(strFN2))
		{
			bool num9 = w9.EPZtoCash(strFN);
			string repXML9 = "";
			if (num9)
			{
				repXML9 = w9.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w9.StatusBarXML(), repXML9);
			w9.Finalization(strFN2);
			return num9;
		}
		w9 = null;
		WebCheck10.ClassFiscal w10 = All.W10;
		if (Operators.CompareString(w10.StatusFN(), "", false) == 0 && w10.Initialization(strFN2))
		{
			bool num10 = w10.EPZtoCash(strFN);
			string repXML10 = "";
			if (num10)
			{
				repXML10 = w10.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w10.StatusBarXML(), repXML10);
			w10.Finalization(strFN2);
			return num10;
		}
		w10 = null;
		WebCheck11.ClassFiscal w11 = All.W11;
		if (Operators.CompareString(w11.StatusFN(), "", false) == 0 && w11.Initialization(strFN2))
		{
			bool num11 = w11.EPZtoCash(strFN);
			string repXML11 = "";
			if (num11)
			{
				repXML11 = w11.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w11.StatusBarXML(), repXML11);
			w11.Finalization(strFN2);
			return num11;
		}
		w11 = null;
		WebCheck12.ClassFiscal w12 = All.W12;
		if (Operators.CompareString(w12.StatusFN(), "", false) == 0 && w12.Initialization(strFN2))
		{
			bool num12 = w12.EPZtoCash(strFN);
			string repXML12 = "";
			if (num12)
			{
				repXML12 = w12.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w12.StatusBarXML(), repXML12);
			w12.Finalization(strFN2);
			return num12;
		}
		w12 = null;
		WebCheck13.ClassFiscal w13 = All.W13;
		if (Operators.CompareString(w13.StatusFN(), "", false) == 0 && w13.Initialization(strFN2))
		{
			bool num13 = w13.EPZtoCash(strFN);
			string repXML13 = "";
			if (num13)
			{
				repXML13 = w13.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w13.StatusBarXML(), repXML13);
			w13.Finalization(strFN2);
			return num13;
		}
		w13 = null;
		WebCheck14.ClassFiscal w14 = All.W14;
		if (Operators.CompareString(w14.StatusFN(), "", false) == 0 && w14.Initialization(strFN2))
		{
			bool num14 = w14.EPZtoCash(strFN);
			string repXML14 = "";
			if (num14)
			{
				repXML14 = w14.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w14.StatusBarXML(), repXML14);
			w14.Finalization(strFN2);
			return num14;
		}
		w14 = null;
		WebCheck15.ClassFiscal w15 = All.W15;
		if (Operators.CompareString(w15.StatusFN(), "", false) == 0 && w15.Initialization(strFN2))
		{
			bool num15 = w15.EPZtoCash(strFN);
			string repXML15 = "";
			if (num15)
			{
				repXML15 = w15.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w15.StatusBarXML(), repXML15);
			w15.Finalization(strFN2);
			return num15;
		}
		w15 = null;
		WebCheck16.ClassFiscal w16 = All.W16;
		if (Operators.CompareString(w16.StatusFN(), "", false) == 0 && w16.Initialization(strFN2))
		{
			bool num16 = w16.EPZtoCash(strFN);
			string repXML16 = "";
			if (num16)
			{
				repXML16 = w16.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w16.StatusBarXML(), repXML16);
			w16.Finalization(strFN2);
			return num16;
		}
		w16 = null;
		WebCheck17.ClassFiscal w17 = All.W17;
		if (Operators.CompareString(w17.StatusFN(), "", false) == 0 && w17.Initialization(strFN2))
		{
			bool num17 = w17.EPZtoCash(strFN);
			string repXML17 = "";
			if (num17)
			{
				repXML17 = w17.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w17.StatusBarXML(), repXML17);
			w17.Finalization(strFN2);
			return num17;
		}
		w17 = null;
		WebCheck18.ClassFiscal w18 = All.W18;
		if (Operators.CompareString(w18.StatusFN(), "", false) == 0 && w18.Initialization(strFN2))
		{
			bool num18 = w18.EPZtoCash(strFN);
			string repXML18 = "";
			if (num18)
			{
				repXML18 = w18.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w18.StatusBarXML(), repXML18);
			w18.Finalization(strFN2);
			return num18;
		}
		w18 = null;
		WebCheck19.ClassFiscal w19 = All.W19;
		if (Operators.CompareString(w19.StatusFN(), "", false) == 0 && w19.Initialization(strFN2))
		{
			bool num19 = w19.EPZtoCash(strFN);
			string repXML19 = "";
			if (num19)
			{
				repXML19 = w19.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w19.StatusBarXML(), repXML19);
			w19.Finalization(strFN2);
			return num19;
		}
		w19 = null;
		WebCheck20.ClassFiscal w20 = All.W20;
		if (Operators.CompareString(w20.StatusFN(), "", false) == 0 && w20.Initialization(strFN2))
		{
			bool num20 = w20.EPZtoCash(strFN);
			string repXML20 = "";
			if (num20)
			{
				repXML20 = w20.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w20.StatusBarXML(), repXML20);
			w20.Finalization(strFN2);
			return num20;
		}
		w20 = null;
		WebCheck21.ClassFiscal w21 = All.W21;
		if (Operators.CompareString(w21.StatusFN(), "", false) == 0 && w21.Initialization(strFN2))
		{
			bool num21 = w21.EPZtoCash(strFN);
			string repXML21 = "";
			if (num21)
			{
				repXML21 = w21.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w21.StatusBarXML(), repXML21);
			w21.Finalization(strFN2);
			return num21;
		}
		w21 = null;
		WebCheck22.ClassFiscal w22 = All.W22;
		if (Operators.CompareString(w22.StatusFN(), "", false) == 0 && w22.Initialization(strFN2))
		{
			bool num22 = w22.EPZtoCash(strFN);
			string repXML22 = "";
			if (num22)
			{
				repXML22 = w22.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w22.StatusBarXML(), repXML22);
			w22.Finalization(strFN2);
			return num22;
		}
		w22 = null;
		WebCheck23.ClassFiscal w23 = All.W23;
		if (Operators.CompareString(w23.StatusFN(), "", false) == 0 && w23.Initialization(strFN2))
		{
			bool num23 = w23.EPZtoCash(strFN);
			string repXML23 = "";
			if (num23)
			{
				repXML23 = w23.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w23.StatusBarXML(), repXML23);
			w23.Finalization(strFN2);
			return num23;
		}
		w23 = null;
		WebCheck24.ClassFiscal w24 = All.W24;
		if (Operators.CompareString(w24.StatusFN(), "", false) == 0 && w24.Initialization(strFN2))
		{
			bool num24 = w24.EPZtoCash(strFN);
			string repXML24 = "";
			if (num24)
			{
				repXML24 = w24.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w24.StatusBarXML(), repXML24);
			w24.Finalization(strFN2);
			return num24;
		}
		w24 = null;
		WebCheck25.ClassFiscal w25 = All.W25;
		if (Operators.CompareString(w25.StatusFN(), "", false) == 0 && w25.Initialization(strFN2))
		{
			bool num25 = w25.EPZtoCash(strFN);
			string repXML25 = "";
			if (num25)
			{
				repXML25 = w25.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w25.StatusBarXML(), repXML25);
			w25.Finalization(strFN2);
			return num25;
		}
		w25 = null;
		WebCheck26.ClassFiscal w26 = All.W26;
		if (Operators.CompareString(w26.StatusFN(), "", false) == 0 && w26.Initialization(strFN2))
		{
			bool num26 = w26.EPZtoCash(strFN);
			string repXML26 = "";
			if (num26)
			{
				repXML26 = w26.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w26.StatusBarXML(), repXML26);
			w26.Finalization(strFN2);
			return num26;
		}
		w26 = null;
		WebCheck27.ClassFiscal w27 = All.W27;
		if (Operators.CompareString(w27.StatusFN(), "", false) == 0 && w27.Initialization(strFN2))
		{
			bool num27 = w27.EPZtoCash(strFN);
			string repXML27 = "";
			if (num27)
			{
				repXML27 = w27.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w27.StatusBarXML(), repXML27);
			w27.Finalization(strFN2);
			return num27;
		}
		w27 = null;
		All.Log.SaveTextToLog(typErrStr.FN, "EPZtoCash", strFN, "Все слоты заняты транзакциями с сервером налоговой");
		All.ReplyRemember(typErrStr.FN, All.XMLRiplyErr(typErrStr.FN, "Все слоты заняты транзакциями с сервером налоговой"));
		return false;
	}

	public bool ReportZ(string strFN)
	{
		checked
		{
			All.zS++;
			TypErrStr typErrStr = All.TestFN(strFN);
			if (typErrStr.errCode > 0)
			{
				All.ReplyRemember(typErrStr.FN, All.XMLRiplyErr(typErrStr.FN, typErrStr.errStr));
				All.Log.SaveTextToLog(typErrStr.FN, "ReportZ", strFN, typErrStr.errStr);
				return false;
			}
			string strFN2 = "<InputParameters><Parameters FN='" + typErrStr.FN + "'/></InputParameters>";
			WebCheck1.ClassFiscal w = All.W1;
			if (Operators.CompareString(w.StatusFN(), "", false) == 0 && w.Initialization(strFN2))
			{
				bool num = w.ReportZ(strFN);
				string repXML = "";
				if (num)
				{
					repXML = w.CheckLine.CheckXML("");
				}
				All.ReplyRemember(typErrStr.FN, w.StatusBarXML(), repXML);
				w.Finalization(strFN2);
				return num;
			}
			w = null;
			WebCheck2.ClassFiscal w2 = All.W2;
			if (Operators.CompareString(w2.StatusFN(), "", false) == 0 && w2.Initialization(strFN2))
			{
				bool num2 = w2.ReportZ(strFN);
				string repXML2 = "";
				if (num2)
				{
					repXML2 = w2.CheckLine.CheckXML("");
				}
				All.ReplyRemember(typErrStr.FN, w2.StatusBarXML(), repXML2);
				w2.Finalization(strFN2);
				return num2;
			}
			w2 = null;
			WebCheck3.ClassFiscal w3 = All.W3;
			if (Operators.CompareString(w3.StatusFN(), "", false) == 0 && w3.Initialization(strFN2))
			{
				bool num3 = w3.ReportZ(strFN);
				string repXML3 = "";
				if (num3)
				{
					repXML3 = w3.CheckLine.CheckXML("");
				}
				All.ReplyRemember(typErrStr.FN, w3.StatusBarXML(), repXML3);
				w3.Finalization(strFN2);
				return num3;
			}
			w3 = null;
			WebCheck4.ClassFiscal w4 = All.W4;
			if (Operators.CompareString(w4.StatusFN(), "", false) == 0 && w4.Initialization(strFN2))
			{
				bool num4 = w4.ReportZ(strFN);
				string repXML4 = "";
				if (num4)
				{
					repXML4 = w4.CheckLine.CheckXML("");
				}
				All.ReplyRemember(typErrStr.FN, w4.StatusBarXML(), repXML4);
				w4.Finalization(strFN2);
				return num4;
			}
			w4 = null;
			WebCheck5.ClassFiscal w5 = All.W5;
			if (Operators.CompareString(w5.StatusFN(), "", false) == 0 && w5.Initialization(strFN2))
			{
				bool num5 = w5.ReportZ(strFN);
				string repXML5 = "";
				if (num5)
				{
					repXML5 = w5.CheckLine.CheckXML("");
				}
				All.ReplyRemember(typErrStr.FN, w5.StatusBarXML(), repXML5);
				w5.Finalization(strFN2);
				return num5;
			}
			w5 = null;
			WebCheck6.ClassFiscal w6 = All.W6;
			if (Operators.CompareString(w6.StatusFN(), "", false) == 0 && w6.Initialization(strFN2))
			{
				bool num6 = w6.ReportZ(strFN);
				string repXML6 = "";
				if (num6)
				{
					repXML6 = w6.CheckLine.CheckXML("");
				}
				All.ReplyRemember(typErrStr.FN, w6.StatusBarXML(), repXML6);
				w6.Finalization(strFN2);
				return num6;
			}
			w6 = null;
			WebCheck7.ClassFiscal w7 = All.W7;
			if (Operators.CompareString(w7.StatusFN(), "", false) == 0 && w7.Initialization(strFN2))
			{
				bool num7 = w7.ReportZ(strFN);
				string repXML7 = "";
				if (num7)
				{
					repXML7 = w7.CheckLine.CheckXML("");
				}
				All.ReplyRemember(typErrStr.FN, w7.StatusBarXML(), repXML7);
				w7.Finalization(strFN2);
				return num7;
			}
			w7 = null;
			WebCheck8.ClassFiscal w8 = All.W8;
			if (Operators.CompareString(w8.StatusFN(), "", false) == 0 && w8.Initialization(strFN2))
			{
				bool num8 = w8.ReportZ(strFN);
				string repXML8 = "";
				if (num8)
				{
					repXML8 = w8.CheckLine.CheckXML("");
				}
				All.ReplyRemember(typErrStr.FN, w8.StatusBarXML(), repXML8);
				w8.Finalization(strFN2);
				return num8;
			}
			w8 = null;
			WebCheck9.ClassFiscal w9 = All.W9;
			if (Operators.CompareString(w9.StatusFN(), "", false) == 0 && w9.Initialization(strFN2))
			{
				bool num9 = w9.ReportZ(strFN);
				string repXML9 = "";
				if (num9)
				{
					repXML9 = w9.CheckLine.CheckXML("");
				}
				All.ReplyRemember(typErrStr.FN, w9.StatusBarXML(), repXML9);
				w9.Finalization(strFN2);
				return num9;
			}
			w9 = null;
			WebCheck10.ClassFiscal w10 = All.W10;
			if (Operators.CompareString(w10.StatusFN(), "", false) == 0 && w10.Initialization(strFN2))
			{
				bool num10 = w10.ReportZ(strFN);
				string repXML10 = "";
				if (num10)
				{
					repXML10 = w10.CheckLine.CheckXML("");
				}
				All.ReplyRemember(typErrStr.FN, w10.StatusBarXML(), repXML10);
				w10.Finalization(strFN2);
				return num10;
			}
			w10 = null;
			WebCheck11.ClassFiscal w11 = All.W11;
			if (Operators.CompareString(w11.StatusFN(), "", false) == 0 && w11.Initialization(strFN2))
			{
				bool num11 = w11.ReportZ(strFN);
				string repXML11 = "";
				if (num11)
				{
					repXML11 = w11.CheckLine.CheckXML("");
				}
				All.ReplyRemember(typErrStr.FN, w11.StatusBarXML(), repXML11);
				w11.Finalization(strFN2);
				return num11;
			}
			w11 = null;
			WebCheck12.ClassFiscal w12 = All.W12;
			if (Operators.CompareString(w12.StatusFN(), "", false) == 0 && w12.Initialization(strFN2))
			{
				bool num12 = w12.ReportZ(strFN);
				string repXML12 = "";
				if (num12)
				{
					repXML12 = w12.CheckLine.CheckXML("");
				}
				All.ReplyRemember(typErrStr.FN, w12.StatusBarXML(), repXML12);
				w12.Finalization(strFN2);
				return num12;
			}
			w12 = null;
			WebCheck13.ClassFiscal w13 = All.W13;
			if (Operators.CompareString(w13.StatusFN(), "", false) == 0 && w13.Initialization(strFN2))
			{
				bool num13 = w13.ReportZ(strFN);
				string repXML13 = "";
				if (num13)
				{
					repXML13 = w13.CheckLine.CheckXML("");
				}
				All.ReplyRemember(typErrStr.FN, w13.StatusBarXML(), repXML13);
				w13.Finalization(strFN2);
				return num13;
			}
			w13 = null;
			WebCheck14.ClassFiscal w14 = All.W14;
			if (Operators.CompareString(w14.StatusFN(), "", false) == 0 && w14.Initialization(strFN2))
			{
				bool num14 = w14.ReportZ(strFN);
				string repXML14 = "";
				if (num14)
				{
					repXML14 = w14.CheckLine.CheckXML("");
				}
				All.ReplyRemember(typErrStr.FN, w14.StatusBarXML(), repXML14);
				w14.Finalization(strFN2);
				return num14;
			}
			w14 = null;
			WebCheck15.ClassFiscal w15 = All.W15;
			if (Operators.CompareString(w15.StatusFN(), "", false) == 0 && w15.Initialization(strFN2))
			{
				bool num15 = w15.ReportZ(strFN);
				string repXML15 = "";
				if (num15)
				{
					repXML15 = w15.CheckLine.CheckXML("");
				}
				All.ReplyRemember(typErrStr.FN, w15.StatusBarXML(), repXML15);
				w15.Finalization(strFN2);
				return num15;
			}
			w15 = null;
			WebCheck16.ClassFiscal w16 = All.W16;
			if (Operators.CompareString(w16.StatusFN(), "", false) == 0 && w16.Initialization(strFN2))
			{
				bool num16 = w16.ReportZ(strFN);
				string repXML16 = "";
				if (num16)
				{
					repXML16 = w16.CheckLine.CheckXML("");
				}
				All.ReplyRemember(typErrStr.FN, w16.StatusBarXML(), repXML16);
				w16.Finalization(strFN2);
				return num16;
			}
			w16 = null;
			WebCheck17.ClassFiscal w17 = All.W17;
			if (Operators.CompareString(w17.StatusFN(), "", false) == 0 && w17.Initialization(strFN2))
			{
				bool num17 = w17.ReportZ(strFN);
				string repXML17 = "";
				if (num17)
				{
					repXML17 = w17.CheckLine.CheckXML("");
				}
				All.ReplyRemember(typErrStr.FN, w17.StatusBarXML(), repXML17);
				w17.Finalization(strFN2);
				return num17;
			}
			w17 = null;
			WebCheck18.ClassFiscal w18 = All.W18;
			if (Operators.CompareString(w18.StatusFN(), "", false) == 0 && w18.Initialization(strFN2))
			{
				bool num18 = w18.ReportZ(strFN);
				string repXML18 = "";
				if (num18)
				{
					repXML18 = w18.CheckLine.CheckXML("");
				}
				All.ReplyRemember(typErrStr.FN, w18.StatusBarXML(), repXML18);
				w18.Finalization(strFN2);
				return num18;
			}
			w18 = null;
			WebCheck19.ClassFiscal w19 = All.W19;
			if (Operators.CompareString(w19.StatusFN(), "", false) == 0 && w19.Initialization(strFN2))
			{
				bool num19 = w19.ReportZ(strFN);
				string repXML19 = "";
				if (num19)
				{
					repXML19 = w19.CheckLine.CheckXML("");
				}
				All.ReplyRemember(typErrStr.FN, w19.StatusBarXML(), repXML19);
				w19.Finalization(strFN2);
				return num19;
			}
			w19 = null;
			WebCheck20.ClassFiscal w20 = All.W20;
			if (Operators.CompareString(w20.StatusFN(), "", false) == 0 && w20.Initialization(strFN2))
			{
				bool num20 = w20.ReportZ(strFN);
				string repXML20 = "";
				if (num20)
				{
					repXML20 = w20.CheckLine.CheckXML("");
				}
				All.ReplyRemember(typErrStr.FN, w20.StatusBarXML(), repXML20);
				w20.Finalization(strFN2);
				return num20;
			}
			w20 = null;
			WebCheck21.ClassFiscal w21 = All.W21;
			if (Operators.CompareString(w21.StatusFN(), "", false) == 0 && w21.Initialization(strFN2))
			{
				bool num21 = w21.ReportZ(strFN);
				string repXML21 = "";
				if (num21)
				{
					repXML21 = w21.CheckLine.CheckXML("");
				}
				All.ReplyRemember(typErrStr.FN, w21.StatusBarXML(), repXML21);
				w21.Finalization(strFN2);
				return num21;
			}
			w21 = null;
			WebCheck22.ClassFiscal w22 = All.W22;
			if (Operators.CompareString(w22.StatusFN(), "", false) == 0 && w22.Initialization(strFN2))
			{
				bool num22 = w22.ReportZ(strFN);
				string repXML22 = "";
				if (num22)
				{
					repXML22 = w22.CheckLine.CheckXML("");
				}
				All.ReplyRemember(typErrStr.FN, w22.StatusBarXML(), repXML22);
				w22.Finalization(strFN2);
				return num22;
			}
			w22 = null;
			WebCheck23.ClassFiscal w23 = All.W23;
			if (Operators.CompareString(w23.StatusFN(), "", false) == 0 && w23.Initialization(strFN2))
			{
				bool num23 = w23.ReportZ(strFN);
				string repXML23 = "";
				if (num23)
				{
					repXML23 = w23.CheckLine.CheckXML("");
				}
				All.ReplyRemember(typErrStr.FN, w23.StatusBarXML(), repXML23);
				w23.Finalization(strFN2);
				return num23;
			}
			w23 = null;
			WebCheck24.ClassFiscal w24 = All.W24;
			if (Operators.CompareString(w24.StatusFN(), "", false) == 0 && w24.Initialization(strFN2))
			{
				bool num24 = w24.ReportZ(strFN);
				string repXML24 = "";
				if (num24)
				{
					repXML24 = w24.CheckLine.CheckXML("");
				}
				All.ReplyRemember(typErrStr.FN, w24.StatusBarXML(), repXML24);
				w24.Finalization(strFN2);
				return num24;
			}
			w24 = null;
			WebCheck25.ClassFiscal w25 = All.W25;
			if (Operators.CompareString(w25.StatusFN(), "", false) == 0 && w25.Initialization(strFN2))
			{
				bool num25 = w25.ReportZ(strFN);
				string repXML25 = "";
				if (num25)
				{
					repXML25 = w25.CheckLine.CheckXML("");
				}
				All.ReplyRemember(typErrStr.FN, w25.StatusBarXML(), repXML25);
				w25.Finalization(strFN2);
				return num25;
			}
			w25 = null;
			WebCheck26.ClassFiscal w26 = All.W26;
			if (Operators.CompareString(w26.StatusFN(), "", false) == 0 && w26.Initialization(strFN2))
			{
				bool num26 = w26.ReportZ(strFN);
				string repXML26 = "";
				if (num26)
				{
					repXML26 = w26.CheckLine.CheckXML("");
				}
				All.ReplyRemember(typErrStr.FN, w26.StatusBarXML(), repXML26);
				w26.Finalization(strFN2);
				return num26;
			}
			w26 = null;
			WebCheck27.ClassFiscal w27 = All.W27;
			if (Operators.CompareString(w27.StatusFN(), "", false) == 0 && w27.Initialization(strFN2))
			{
				bool num27 = w27.ReportZ(strFN);
				string repXML27 = "";
				if (num27)
				{
					repXML27 = w27.CheckLine.CheckXML("");
				}
				All.ReplyRemember(typErrStr.FN, w27.StatusBarXML(), repXML27);
				w27.Finalization(strFN2);
				return num27;
			}
			w27 = null;
			WebCheck28.ClassFiscal w28 = All.W28;
			if (Operators.CompareString(w28.StatusFN(), "", false) == 0 && w28.Initialization(strFN2))
			{
				bool num28 = w28.ReportZ(strFN);
				string repXML28 = "";
				if (num28)
				{
					repXML28 = w28.CheckLine.CheckXML("");
				}
				All.ReplyRemember(typErrStr.FN, w28.StatusBarXML(), repXML28);
				w28.Finalization(strFN2);
				return num28;
			}
			w28 = null;
			WebCheck29.ClassFiscal w29 = All.W29;
			if (Operators.CompareString(w29.StatusFN(), "", false) == 0 && w29.Initialization(strFN2))
			{
				bool num29 = w29.ReportZ(strFN);
				string repXML29 = "";
				if (num29)
				{
					repXML29 = w29.CheckLine.CheckXML("");
				}
				All.ReplyRemember(typErrStr.FN, w29.StatusBarXML(), repXML29);
				w29.Finalization(strFN2);
				return num29;
			}
			w29 = null;
			WebCheck30.ClassFiscal w30 = All.W30;
			if (Operators.CompareString(w30.StatusFN(), "", false) == 0 && w30.Initialization(strFN2))
			{
				bool num30 = w30.ReportZ(strFN);
				string repXML30 = "";
				if (num30)
				{
					repXML30 = w30.CheckLine.CheckXML("");
				}
				All.ReplyRemember(typErrStr.FN, w30.StatusBarXML(), repXML30);
				w30.Finalization(strFN2);
				return num30;
			}
			w30 = null;
			All.zF++;
			string text = All.zS + "/" + All.zF;
			All.Log.SaveTextToLog(typErrStr.FN, "ReportZ " + text, strFN, "Все слоты заняты транзакциями с сервером налоговой");
			All.ReplyRemember(typErrStr.FN, All.XMLRiplyErr(typErrStr.FN, "Все слоты заняты транзакциями с сервером налоговой"));
			return false;
		}
	}

	public bool ReportX(string strFN)
	{
		checked
		{
			All.xS++;
			TypErrStr typErrStr = All.TestFN(strFN);
			if (typErrStr.errCode > 0)
			{
				All.ReplyRemember(typErrStr.FN, All.XMLRiplyErr(typErrStr.FN, typErrStr.errStr));
				All.Log.SaveTextToLog(typErrStr.FN, "ReportX", strFN, typErrStr.errStr);
				return false;
			}
			string strFN2 = "<InputParameters><Parameters FN='" + typErrStr.FN + "'/></InputParameters>";
			WebCheck1.ClassFiscal w = All.W1;
			if (Operators.CompareString(w.StatusFN(), "", false) == 0 && w.Initialization(strFN2))
			{
				bool num = w.ReportX(strFN);
				string repXML = "";
				if (num)
				{
					repXML = w.CheckLine.CheckArrayToXML();
				}
				All.ReplyRemember(typErrStr.FN, w.StatusBarXML(), repXML);
				w.Finalization(strFN2);
				return num;
			}
			w = null;
			WebCheck2.ClassFiscal w2 = All.W2;
			if (Operators.CompareString(w2.StatusFN(), "", false) == 0 && w2.Initialization(strFN2))
			{
				bool num2 = w2.ReportX(strFN);
				string repXML2 = "";
				if (num2)
				{
					repXML2 = w2.CheckLine.CheckArrayToXML();
				}
				All.ReplyRemember(typErrStr.FN, w2.StatusBarXML(), repXML2);
				w2.Finalization(strFN2);
				return num2;
			}
			w2 = null;
			WebCheck3.ClassFiscal w3 = All.W3;
			if (Operators.CompareString(w3.StatusFN(), "", false) == 0 && w3.Initialization(strFN2))
			{
				bool num3 = w3.ReportX(strFN);
				string repXML3 = "";
				if (num3)
				{
					repXML3 = w3.CheckLine.CheckArrayToXML();
				}
				All.ReplyRemember(typErrStr.FN, w3.StatusBarXML(), repXML3);
				w3.Finalization(strFN2);
				return num3;
			}
			w3 = null;
			WebCheck4.ClassFiscal w4 = All.W4;
			if (Operators.CompareString(w4.StatusFN(), "", false) == 0 && w4.Initialization(strFN2))
			{
				bool num4 = w4.ReportX(strFN);
				string repXML4 = "";
				if (num4)
				{
					repXML4 = w4.CheckLine.CheckArrayToXML();
				}
				All.ReplyRemember(typErrStr.FN, w4.StatusBarXML(), repXML4);
				w4.Finalization(strFN2);
				return num4;
			}
			w4 = null;
			WebCheck5.ClassFiscal w5 = All.W5;
			if (Operators.CompareString(w5.StatusFN(), "", false) == 0 && w5.Initialization(strFN2))
			{
				bool num5 = w5.ReportX(strFN);
				string repXML5 = "";
				if (num5)
				{
					repXML5 = w5.CheckLine.CheckArrayToXML();
				}
				All.ReplyRemember(typErrStr.FN, w5.StatusBarXML(), repXML5);
				w5.Finalization(strFN2);
				return num5;
			}
			w5 = null;
			WebCheck6.ClassFiscal w6 = All.W6;
			if (Operators.CompareString(w6.StatusFN(), "", false) == 0 && w6.Initialization(strFN2))
			{
				bool num6 = w6.ReportX(strFN);
				string repXML6 = "";
				if (num6)
				{
					repXML6 = w6.CheckLine.CheckArrayToXML();
				}
				All.ReplyRemember(typErrStr.FN, w6.StatusBarXML(), repXML6);
				w6.Finalization(strFN2);
				return num6;
			}
			w6 = null;
			WebCheck7.ClassFiscal w7 = All.W7;
			if (Operators.CompareString(w7.StatusFN(), "", false) == 0 && w7.Initialization(strFN2))
			{
				bool num7 = w7.ReportX(strFN);
				string repXML7 = "";
				if (num7)
				{
					repXML7 = w7.CheckLine.CheckArrayToXML();
				}
				All.ReplyRemember(typErrStr.FN, w7.StatusBarXML(), repXML7);
				w7.Finalization(strFN2);
				return num7;
			}
			w7 = null;
			WebCheck8.ClassFiscal w8 = All.W8;
			if (Operators.CompareString(w8.StatusFN(), "", false) == 0 && w8.Initialization(strFN2))
			{
				bool num8 = w8.ReportX(strFN);
				string repXML8 = "";
				if (num8)
				{
					repXML8 = w8.CheckLine.CheckArrayToXML();
				}
				All.ReplyRemember(typErrStr.FN, w8.StatusBarXML(), repXML8);
				w8.Finalization(strFN2);
				return num8;
			}
			w8 = null;
			WebCheck9.ClassFiscal w9 = All.W9;
			if (Operators.CompareString(w9.StatusFN(), "", false) == 0 && w9.Initialization(strFN2))
			{
				bool num9 = w9.ReportX(strFN);
				string repXML9 = "";
				if (num9)
				{
					repXML9 = w9.CheckLine.CheckArrayToXML();
				}
				All.ReplyRemember(typErrStr.FN, w9.StatusBarXML(), repXML9);
				w9.Finalization(strFN2);
				return num9;
			}
			w9 = null;
			WebCheck10.ClassFiscal w10 = All.W10;
			if (Operators.CompareString(w10.StatusFN(), "", false) == 0 && w10.Initialization(strFN2))
			{
				bool num10 = w10.ReportX(strFN);
				string repXML10 = "";
				if (num10)
				{
					repXML10 = w10.CheckLine.CheckArrayToXML();
				}
				All.ReplyRemember(typErrStr.FN, w10.StatusBarXML(), repXML10);
				w10.Finalization(strFN2);
				return num10;
			}
			w10 = null;
			WebCheck11.ClassFiscal w11 = All.W11;
			if (Operators.CompareString(w11.StatusFN(), "", false) == 0 && w11.Initialization(strFN2))
			{
				bool num11 = w11.ReportX(strFN);
				string repXML11 = "";
				if (num11)
				{
					repXML11 = w11.CheckLine.CheckArrayToXML();
				}
				All.ReplyRemember(typErrStr.FN, w11.StatusBarXML(), repXML11);
				w11.Finalization(strFN2);
				return num11;
			}
			w11 = null;
			WebCheck12.ClassFiscal w12 = All.W12;
			if (Operators.CompareString(w12.StatusFN(), "", false) == 0 && w12.Initialization(strFN2))
			{
				bool num12 = w12.ReportX(strFN);
				string repXML12 = "";
				if (num12)
				{
					repXML12 = w12.CheckLine.CheckArrayToXML();
				}
				All.ReplyRemember(typErrStr.FN, w12.StatusBarXML(), repXML12);
				w12.Finalization(strFN2);
				return num12;
			}
			w12 = null;
			WebCheck13.ClassFiscal w13 = All.W13;
			if (Operators.CompareString(w13.StatusFN(), "", false) == 0 && w13.Initialization(strFN2))
			{
				bool num13 = w13.ReportX(strFN);
				string repXML13 = "";
				if (num13)
				{
					repXML13 = w13.CheckLine.CheckArrayToXML();
				}
				All.ReplyRemember(typErrStr.FN, w13.StatusBarXML(), repXML13);
				w13.Finalization(strFN2);
				return num13;
			}
			w13 = null;
			WebCheck14.ClassFiscal w14 = All.W14;
			if (Operators.CompareString(w14.StatusFN(), "", false) == 0 && w14.Initialization(strFN2))
			{
				bool num14 = w14.ReportX(strFN);
				string repXML14 = "";
				if (num14)
				{
					repXML14 = w14.CheckLine.CheckArrayToXML();
				}
				All.ReplyRemember(typErrStr.FN, w14.StatusBarXML(), repXML14);
				w14.Finalization(strFN2);
				return num14;
			}
			w14 = null;
			WebCheck15.ClassFiscal w15 = All.W15;
			if (Operators.CompareString(w15.StatusFN(), "", false) == 0 && w15.Initialization(strFN2))
			{
				bool num15 = w15.ReportX(strFN);
				string repXML15 = "";
				if (num15)
				{
					repXML15 = w15.CheckLine.CheckArrayToXML();
				}
				All.ReplyRemember(typErrStr.FN, w15.StatusBarXML(), repXML15);
				w15.Finalization(strFN2);
				return num15;
			}
			w15 = null;
			WebCheck16.ClassFiscal w16 = All.W16;
			if (Operators.CompareString(w16.StatusFN(), "", false) == 0 && w16.Initialization(strFN2))
			{
				bool num16 = w16.ReportX(strFN);
				string repXML16 = "";
				if (num16)
				{
					repXML16 = w16.CheckLine.CheckArrayToXML();
				}
				All.ReplyRemember(typErrStr.FN, w16.StatusBarXML(), repXML16);
				w16.Finalization(strFN2);
				return num16;
			}
			w16 = null;
			WebCheck17.ClassFiscal w17 = All.W17;
			if (Operators.CompareString(w17.StatusFN(), "", false) == 0 && w17.Initialization(strFN2))
			{
				bool num17 = w17.ReportX(strFN);
				string repXML17 = "";
				if (num17)
				{
					repXML17 = w17.CheckLine.CheckArrayToXML();
				}
				All.ReplyRemember(typErrStr.FN, w17.StatusBarXML(), repXML17);
				w17.Finalization(strFN2);
				return num17;
			}
			w17 = null;
			WebCheck18.ClassFiscal w18 = All.W18;
			if (Operators.CompareString(w18.StatusFN(), "", false) == 0 && w18.Initialization(strFN2))
			{
				bool num18 = w18.ReportX(strFN);
				string repXML18 = "";
				if (num18)
				{
					repXML18 = w18.CheckLine.CheckArrayToXML();
				}
				All.ReplyRemember(typErrStr.FN, w18.StatusBarXML(), repXML18);
				w18.Finalization(strFN2);
				return num18;
			}
			w18 = null;
			WebCheck19.ClassFiscal w19 = All.W19;
			if (Operators.CompareString(w19.StatusFN(), "", false) == 0 && w19.Initialization(strFN2))
			{
				bool num19 = w19.ReportX(strFN);
				string repXML19 = "";
				if (num19)
				{
					repXML19 = w19.CheckLine.CheckArrayToXML();
				}
				All.ReplyRemember(typErrStr.FN, w19.StatusBarXML(), repXML19);
				w19.Finalization(strFN2);
				return num19;
			}
			w19 = null;
			WebCheck20.ClassFiscal w20 = All.W20;
			if (Operators.CompareString(w20.StatusFN(), "", false) == 0 && w20.Initialization(strFN2))
			{
				bool num20 = w20.ReportX(strFN);
				string repXML20 = "";
				if (num20)
				{
					repXML20 = w20.CheckLine.CheckArrayToXML();
				}
				All.ReplyRemember(typErrStr.FN, w20.StatusBarXML(), repXML20);
				w20.Finalization(strFN2);
				return num20;
			}
			w20 = null;
			WebCheck21.ClassFiscal w21 = All.W21;
			if (Operators.CompareString(w21.StatusFN(), "", false) == 0 && w21.Initialization(strFN2))
			{
				bool num21 = w21.ReportX(strFN);
				string repXML21 = "";
				if (num21)
				{
					repXML21 = w21.CheckLine.CheckArrayToXML();
				}
				All.ReplyRemember(typErrStr.FN, w21.StatusBarXML(), repXML21);
				w21.Finalization(strFN2);
				return num21;
			}
			w21 = null;
			WebCheck22.ClassFiscal w22 = All.W22;
			if (Operators.CompareString(w22.StatusFN(), "", false) == 0 && w22.Initialization(strFN2))
			{
				bool num22 = w22.ReportX(strFN);
				string repXML22 = "";
				if (num22)
				{
					repXML22 = w22.CheckLine.CheckArrayToXML();
				}
				All.ReplyRemember(typErrStr.FN, w22.StatusBarXML(), repXML22);
				w22.Finalization(strFN2);
				return num22;
			}
			w22 = null;
			WebCheck23.ClassFiscal w23 = All.W23;
			if (Operators.CompareString(w23.StatusFN(), "", false) == 0 && w23.Initialization(strFN2))
			{
				bool num23 = w23.ReportX(strFN);
				string repXML23 = "";
				if (num23)
				{
					repXML23 = w23.CheckLine.CheckArrayToXML();
				}
				All.ReplyRemember(typErrStr.FN, w23.StatusBarXML(), repXML23);
				w23.Finalization(strFN2);
				return num23;
			}
			w23 = null;
			WebCheck24.ClassFiscal w24 = All.W24;
			if (Operators.CompareString(w24.StatusFN(), "", false) == 0 && w24.Initialization(strFN2))
			{
				bool num24 = w24.ReportX(strFN);
				string repXML24 = "";
				if (num24)
				{
					repXML24 = w24.CheckLine.CheckArrayToXML();
				}
				All.ReplyRemember(typErrStr.FN, w24.StatusBarXML(), repXML24);
				w24.Finalization(strFN2);
				return num24;
			}
			w24 = null;
			WebCheck25.ClassFiscal w25 = All.W25;
			if (Operators.CompareString(w25.StatusFN(), "", false) == 0 && w25.Initialization(strFN2))
			{
				bool num25 = w25.ReportX(strFN);
				string repXML25 = "";
				if (num25)
				{
					repXML25 = w25.CheckLine.CheckArrayToXML();
				}
				All.ReplyRemember(typErrStr.FN, w25.StatusBarXML(), repXML25);
				w25.Finalization(strFN2);
				return num25;
			}
			w25 = null;
			WebCheck26.ClassFiscal w26 = All.W26;
			if (Operators.CompareString(w26.StatusFN(), "", false) == 0 && w26.Initialization(strFN2))
			{
				bool num26 = w26.ReportX(strFN);
				string repXML26 = "";
				if (num26)
				{
					repXML26 = w26.CheckLine.CheckArrayToXML();
				}
				All.ReplyRemember(typErrStr.FN, w26.StatusBarXML(), repXML26);
				w26.Finalization(strFN2);
				return num26;
			}
			w26 = null;
			WebCheck27.ClassFiscal w27 = All.W27;
			if (Operators.CompareString(w27.StatusFN(), "", false) == 0 && w27.Initialization(strFN2))
			{
				bool num27 = w27.ReportX(strFN);
				string repXML27 = "";
				if (num27)
				{
					repXML27 = w27.CheckLine.CheckArrayToXML();
				}
				All.ReplyRemember(typErrStr.FN, w27.StatusBarXML(), repXML27);
				w27.Finalization(strFN2);
				return num27;
			}
			w27 = null;
			WebCheck28.ClassFiscal w28 = All.W28;
			if (Operators.CompareString(w28.StatusFN(), "", false) == 0 && w28.Initialization(strFN2))
			{
				bool num28 = w28.ReportX(strFN);
				string repXML28 = "";
				if (num28)
				{
					repXML28 = w28.CheckLine.CheckArrayToXML();
				}
				All.ReplyRemember(typErrStr.FN, w28.StatusBarXML(), repXML28);
				w28.Finalization(strFN2);
				return num28;
			}
			w28 = null;
			WebCheck29.ClassFiscal w29 = All.W29;
			if (Operators.CompareString(w29.StatusFN(), "", false) == 0 && w29.Initialization(strFN2))
			{
				bool num29 = w29.ReportX(strFN);
				string repXML29 = "";
				if (num29)
				{
					repXML29 = w29.CheckLine.CheckArrayToXML();
				}
				All.ReplyRemember(typErrStr.FN, w29.StatusBarXML(), repXML29);
				w29.Finalization(strFN2);
				return num29;
			}
			w29 = null;
			WebCheck30.ClassFiscal w30 = All.W30;
			if (Operators.CompareString(w30.StatusFN(), "", false) == 0 && w30.Initialization(strFN2))
			{
				bool num30 = w30.ReportX(strFN);
				string repXML30 = "";
				if (num30)
				{
					repXML30 = w30.CheckLine.CheckArrayToXML();
				}
				All.ReplyRemember(typErrStr.FN, w30.StatusBarXML(), repXML30);
				w30.Finalization(strFN2);
				return num30;
			}
			w30 = null;
			All.xF++;
			string text = All.xS + "/" + All.xF;
			All.Log.SaveTextToLog(typErrStr.FN, "ReportX " + text, strFN, "Все слоты заняты транзакциями с сервером налоговой");
			All.ReplyRemember(typErrStr.FN, All.XMLRiplyErr(typErrStr.FN, "Все слоты заняты транзакциями с сервером налоговой"));
			return false;
		}
	}

	public bool CashInOut(string strFN)
	{
		TypErrStr typErrStr = All.TestFN(strFN);
		if (typErrStr.errCode > 0)
		{
			All.ReplyRemember(typErrStr.FN, All.XMLRiplyErr(typErrStr.FN, typErrStr.errStr));
			All.Log.SaveTextToLog(typErrStr.FN, "CashInOut", strFN, typErrStr.errStr);
			return false;
		}
		string strFN2 = "<InputParameters><Parameters FN='" + typErrStr.FN + "'/></InputParameters>";
		WebCheck1.ClassFiscal w = All.W1;
		if (Operators.CompareString(w.StatusFN(), "", false) == 0 && w.Initialization(strFN2))
		{
			bool num = w.CashInOut(strFN);
			string repXML = "";
			if (num)
			{
				repXML = w.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w.StatusBarXML(), repXML);
			w.Finalization(strFN2);
			return num;
		}
		w = null;
		WebCheck2.ClassFiscal w2 = All.W2;
		if (Operators.CompareString(w2.StatusFN(), "", false) == 0 && w2.Initialization(strFN2))
		{
			bool num2 = w2.CashInOut(strFN);
			string repXML2 = "";
			if (num2)
			{
				repXML2 = w2.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w2.StatusBarXML(), repXML2);
			w2.Finalization(strFN2);
			return num2;
		}
		w2 = null;
		WebCheck3.ClassFiscal w3 = All.W3;
		if (Operators.CompareString(w3.StatusFN(), "", false) == 0 && w3.Initialization(strFN2))
		{
			bool num3 = w3.CashInOut(strFN);
			string repXML3 = "";
			if (num3)
			{
				repXML3 = w3.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w3.StatusBarXML(), repXML3);
			w3.Finalization(strFN2);
			return num3;
		}
		w3 = null;
		WebCheck4.ClassFiscal w4 = All.W4;
		if (Operators.CompareString(w4.StatusFN(), "", false) == 0 && w4.Initialization(strFN2))
		{
			bool num4 = w4.CashInOut(strFN);
			string repXML4 = "";
			if (num4)
			{
				repXML4 = w4.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w4.StatusBarXML(), repXML4);
			w4.Finalization(strFN2);
			return num4;
		}
		w4 = null;
		WebCheck5.ClassFiscal w5 = All.W5;
		if (Operators.CompareString(w5.StatusFN(), "", false) == 0 && w5.Initialization(strFN2))
		{
			bool num5 = w5.CashInOut(strFN);
			string repXML5 = "";
			if (num5)
			{
				repXML5 = w5.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w5.StatusBarXML(), repXML5);
			w5.Finalization(strFN2);
			return num5;
		}
		w5 = null;
		WebCheck6.ClassFiscal w6 = All.W6;
		if (Operators.CompareString(w6.StatusFN(), "", false) == 0 && w6.Initialization(strFN2))
		{
			bool num6 = w6.CashInOut(strFN);
			string repXML6 = "";
			if (num6)
			{
				repXML6 = w6.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w6.StatusBarXML(), repXML6);
			w6.Finalization(strFN2);
			return num6;
		}
		w6 = null;
		WebCheck7.ClassFiscal w7 = All.W7;
		if (Operators.CompareString(w7.StatusFN(), "", false) == 0 && w7.Initialization(strFN2))
		{
			bool num7 = w7.CashInOut(strFN);
			string repXML7 = "";
			if (num7)
			{
				repXML7 = w7.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w7.StatusBarXML(), repXML7);
			w7.Finalization(strFN2);
			return num7;
		}
		w7 = null;
		WebCheck8.ClassFiscal w8 = All.W8;
		if (Operators.CompareString(w8.StatusFN(), "", false) == 0 && w8.Initialization(strFN2))
		{
			bool num8 = w8.CashInOut(strFN);
			string repXML8 = "";
			if (num8)
			{
				repXML8 = w8.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w8.StatusBarXML(), repXML8);
			w8.Finalization(strFN2);
			return num8;
		}
		w8 = null;
		WebCheck9.ClassFiscal w9 = All.W9;
		if (Operators.CompareString(w9.StatusFN(), "", false) == 0 && w9.Initialization(strFN2))
		{
			bool num9 = w9.CashInOut(strFN);
			string repXML9 = "";
			if (num9)
			{
				repXML9 = w9.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w9.StatusBarXML(), repXML9);
			w9.Finalization(strFN2);
			return num9;
		}
		w9 = null;
		WebCheck10.ClassFiscal w10 = All.W10;
		if (Operators.CompareString(w10.StatusFN(), "", false) == 0 && w10.Initialization(strFN2))
		{
			bool num10 = w10.CashInOut(strFN);
			string repXML10 = "";
			if (num10)
			{
				repXML10 = w10.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w10.StatusBarXML(), repXML10);
			w10.Finalization(strFN2);
			return num10;
		}
		w10 = null;
		WebCheck11.ClassFiscal w11 = All.W11;
		if (Operators.CompareString(w11.StatusFN(), "", false) == 0 && w11.Initialization(strFN2))
		{
			bool num11 = w11.CashInOut(strFN);
			string repXML11 = "";
			if (num11)
			{
				repXML11 = w11.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w11.StatusBarXML(), repXML11);
			w11.Finalization(strFN2);
			return num11;
		}
		w11 = null;
		WebCheck12.ClassFiscal w12 = All.W12;
		if (Operators.CompareString(w12.StatusFN(), "", false) == 0 && w12.Initialization(strFN2))
		{
			bool num12 = w12.CashInOut(strFN);
			string repXML12 = "";
			if (num12)
			{
				repXML12 = w12.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w12.StatusBarXML(), repXML12);
			w12.Finalization(strFN2);
			return num12;
		}
		w12 = null;
		WebCheck13.ClassFiscal w13 = All.W13;
		if (Operators.CompareString(w13.StatusFN(), "", false) == 0 && w13.Initialization(strFN2))
		{
			bool num13 = w13.CashInOut(strFN);
			string repXML13 = "";
			if (num13)
			{
				repXML13 = w13.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w13.StatusBarXML(), repXML13);
			w13.Finalization(strFN2);
			return num13;
		}
		w13 = null;
		WebCheck14.ClassFiscal w14 = All.W14;
		if (Operators.CompareString(w14.StatusFN(), "", false) == 0 && w14.Initialization(strFN2))
		{
			bool num14 = w14.CashInOut(strFN);
			string repXML14 = "";
			if (num14)
			{
				repXML14 = w14.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w14.StatusBarXML(), repXML14);
			w14.Finalization(strFN2);
			return num14;
		}
		w14 = null;
		WebCheck15.ClassFiscal w15 = All.W15;
		if (Operators.CompareString(w15.StatusFN(), "", false) == 0 && w15.Initialization(strFN2))
		{
			bool num15 = w15.CashInOut(strFN);
			string repXML15 = "";
			if (num15)
			{
				repXML15 = w15.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w15.StatusBarXML(), repXML15);
			w15.Finalization(strFN2);
			return num15;
		}
		w15 = null;
		WebCheck16.ClassFiscal w16 = All.W16;
		if (Operators.CompareString(w16.StatusFN(), "", false) == 0 && w16.Initialization(strFN2))
		{
			bool num16 = w16.CashInOut(strFN);
			string repXML16 = "";
			if (num16)
			{
				repXML16 = w16.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w16.StatusBarXML(), repXML16);
			w16.Finalization(strFN2);
			return num16;
		}
		w16 = null;
		WebCheck17.ClassFiscal w17 = All.W17;
		if (Operators.CompareString(w17.StatusFN(), "", false) == 0 && w17.Initialization(strFN2))
		{
			bool num17 = w17.CashInOut(strFN);
			string repXML17 = "";
			if (num17)
			{
				repXML17 = w17.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w17.StatusBarXML(), repXML17);
			w17.Finalization(strFN2);
			return num17;
		}
		w17 = null;
		WebCheck18.ClassFiscal w18 = All.W18;
		if (Operators.CompareString(w18.StatusFN(), "", false) == 0 && w18.Initialization(strFN2))
		{
			bool num18 = w18.CashInOut(strFN);
			string repXML18 = "";
			if (num18)
			{
				repXML18 = w18.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w18.StatusBarXML(), repXML18);
			w18.Finalization(strFN2);
			return num18;
		}
		w18 = null;
		WebCheck19.ClassFiscal w19 = All.W19;
		if (Operators.CompareString(w19.StatusFN(), "", false) == 0 && w19.Initialization(strFN2))
		{
			bool num19 = w19.CashInOut(strFN);
			string repXML19 = "";
			if (num19)
			{
				repXML19 = w19.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w19.StatusBarXML(), repXML19);
			w19.Finalization(strFN2);
			return num19;
		}
		w19 = null;
		WebCheck20.ClassFiscal w20 = All.W20;
		if (Operators.CompareString(w20.StatusFN(), "", false) == 0 && w20.Initialization(strFN2))
		{
			bool num20 = w20.CashInOut(strFN);
			string repXML20 = "";
			if (num20)
			{
				repXML20 = w20.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w20.StatusBarXML(), repXML20);
			w20.Finalization(strFN2);
			return num20;
		}
		w20 = null;
		WebCheck21.ClassFiscal w21 = All.W21;
		if (Operators.CompareString(w21.StatusFN(), "", false) == 0 && w21.Initialization(strFN2))
		{
			bool num21 = w21.CashInOut(strFN);
			string repXML21 = "";
			if (num21)
			{
				repXML21 = w21.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w21.StatusBarXML(), repXML21);
			w21.Finalization(strFN2);
			return num21;
		}
		w21 = null;
		WebCheck22.ClassFiscal w22 = All.W22;
		if (Operators.CompareString(w22.StatusFN(), "", false) == 0 && w22.Initialization(strFN2))
		{
			bool num22 = w22.CashInOut(strFN);
			string repXML22 = "";
			if (num22)
			{
				repXML22 = w22.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w22.StatusBarXML(), repXML22);
			w22.Finalization(strFN2);
			return num22;
		}
		w22 = null;
		WebCheck23.ClassFiscal w23 = All.W23;
		if (Operators.CompareString(w23.StatusFN(), "", false) == 0 && w23.Initialization(strFN2))
		{
			bool num23 = w23.CashInOut(strFN);
			string repXML23 = "";
			if (num23)
			{
				repXML23 = w23.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w23.StatusBarXML(), repXML23);
			w23.Finalization(strFN2);
			return num23;
		}
		w23 = null;
		WebCheck24.ClassFiscal w24 = All.W24;
		if (Operators.CompareString(w24.StatusFN(), "", false) == 0 && w24.Initialization(strFN2))
		{
			bool num24 = w24.CashInOut(strFN);
			string repXML24 = "";
			if (num24)
			{
				repXML24 = w24.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w24.StatusBarXML(), repXML24);
			w24.Finalization(strFN2);
			return num24;
		}
		w24 = null;
		WebCheck25.ClassFiscal w25 = All.W25;
		if (Operators.CompareString(w25.StatusFN(), "", false) == 0 && w25.Initialization(strFN2))
		{
			bool num25 = w25.CashInOut(strFN);
			string repXML25 = "";
			if (num25)
			{
				repXML25 = w25.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w25.StatusBarXML(), repXML25);
			w25.Finalization(strFN2);
			return num25;
		}
		w25 = null;
		WebCheck26.ClassFiscal w26 = All.W26;
		if (Operators.CompareString(w26.StatusFN(), "", false) == 0 && w26.Initialization(strFN2))
		{
			bool num26 = w26.CashInOut(strFN);
			string repXML26 = "";
			if (num26)
			{
				repXML26 = w26.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w26.StatusBarXML(), repXML26);
			w26.Finalization(strFN2);
			return num26;
		}
		w26 = null;
		WebCheck27.ClassFiscal w27 = All.W27;
		if (Operators.CompareString(w27.StatusFN(), "", false) == 0 && w27.Initialization(strFN2))
		{
			bool num27 = w27.CashInOut(strFN);
			string repXML27 = "";
			if (num27)
			{
				repXML27 = w27.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w27.StatusBarXML(), repXML27);
			w27.Finalization(strFN2);
			return num27;
		}
		w27 = null;
		WebCheck28.ClassFiscal w28 = All.W28;
		if (Operators.CompareString(w28.StatusFN(), "", false) == 0 && w28.Initialization(strFN2))
		{
			bool num28 = w28.CashInOut(strFN);
			string repXML28 = "";
			if (num28)
			{
				repXML28 = w28.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w28.StatusBarXML(), repXML28);
			w28.Finalization(strFN2);
			return num28;
		}
		w28 = null;
		WebCheck29.ClassFiscal w29 = All.W29;
		if (Operators.CompareString(w29.StatusFN(), "", false) == 0 && w29.Initialization(strFN2))
		{
			bool num29 = w29.CashInOut(strFN);
			string repXML29 = "";
			if (num29)
			{
				repXML29 = w29.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w29.StatusBarXML(), repXML29);
			w29.Finalization(strFN2);
			return num29;
		}
		w29 = null;
		WebCheck30.ClassFiscal w30 = All.W30;
		if (Operators.CompareString(w30.StatusFN(), "", false) == 0 && w30.Initialization(strFN2))
		{
			bool num30 = w30.CashInOut(strFN);
			string repXML30 = "";
			if (num30)
			{
				repXML30 = w30.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w30.StatusBarXML(), repXML30);
			w30.Finalization(strFN2);
			return num30;
		}
		w30 = null;
		All.Log.SaveTextToLog(typErrStr.FN, "CashInOut", strFN, "Все слоты заняты транзакциями с сервером налоговой");
		All.ReplyRemember(typErrStr.FN, All.XMLRiplyErr(typErrStr.FN, "Все слоты заняты транзакциями с сервером налоговой"));
		return false;
	}

	public bool AddPayForm(string strFN)
	{
		TypErrStr typErrStr = All.TestFN(strFN);
		if (typErrStr.errCode > 0)
		{
			All.ReplyRemember(typErrStr.FN, All.XMLRiplyErr(typErrStr.FN, typErrStr.errStr));
			All.Log.SaveTextToLog(typErrStr.FN, "AddPayForm", strFN, typErrStr.errStr);
			return false;
		}
		string strFN2 = "<InputParameters><Parameters FN='" + typErrStr.FN + "'/></InputParameters>";
		WebCheck1.ClassFiscal w = All.W1;
		if (Operators.CompareString(w.StatusFN(), "", false) == 0 && w.Initialization(strFN2))
		{
			bool result = w.AddPayForm(strFN);
			All.ReplyRemember(typErrStr.FN, w.StatusBarXML());
			w.Finalization(strFN2);
			return result;
		}
		w = null;
		WebCheck2.ClassFiscal w2 = All.W2;
		if (Operators.CompareString(w2.StatusFN(), "", false) == 0 && w2.Initialization(strFN2))
		{
			bool result2 = w2.AddPayForm(strFN);
			All.ReplyRemember(typErrStr.FN, w2.StatusBarXML());
			w2.Finalization(strFN2);
			return result2;
		}
		w2 = null;
		WebCheck3.ClassFiscal w3 = All.W3;
		if (Operators.CompareString(w3.StatusFN(), "", false) == 0 && w3.Initialization(strFN2))
		{
			bool result3 = w3.AddPayForm(strFN);
			All.ReplyRemember(typErrStr.FN, w3.StatusBarXML());
			w3.Finalization(strFN2);
			return result3;
		}
		w3 = null;
		WebCheck4.ClassFiscal w4 = All.W4;
		if (Operators.CompareString(w4.StatusFN(), "", false) == 0 && w4.Initialization(strFN2))
		{
			bool result4 = w4.AddPayForm(strFN);
			All.ReplyRemember(typErrStr.FN, w4.StatusBarXML());
			w4.Finalization(strFN2);
			return result4;
		}
		w4 = null;
		WebCheck5.ClassFiscal w5 = All.W5;
		if (Operators.CompareString(w5.StatusFN(), "", false) == 0 && w5.Initialization(strFN2))
		{
			bool result5 = w5.AddPayForm(strFN);
			All.ReplyRemember(typErrStr.FN, w5.StatusBarXML());
			w5.Finalization(strFN2);
			return result5;
		}
		w5 = null;
		WebCheck6.ClassFiscal w6 = All.W6;
		if (Operators.CompareString(w6.StatusFN(), "", false) == 0 && w6.Initialization(strFN2))
		{
			bool result6 = w6.AddPayForm(strFN);
			All.ReplyRemember(typErrStr.FN, w6.StatusBarXML());
			w6.Finalization(strFN2);
			return result6;
		}
		w6 = null;
		WebCheck7.ClassFiscal w7 = All.W7;
		if (Operators.CompareString(w7.StatusFN(), "", false) == 0 && w7.Initialization(strFN2))
		{
			bool result7 = w7.AddPayForm(strFN);
			All.ReplyRemember(typErrStr.FN, w7.StatusBarXML());
			w7.Finalization(strFN2);
			return result7;
		}
		w7 = null;
		WebCheck8.ClassFiscal w8 = All.W8;
		if (Operators.CompareString(w8.StatusFN(), "", false) == 0 && w8.Initialization(strFN2))
		{
			bool result8 = w8.AddPayForm(strFN);
			All.ReplyRemember(typErrStr.FN, w8.StatusBarXML());
			w8.Finalization(strFN2);
			return result8;
		}
		w8 = null;
		WebCheck9.ClassFiscal w9 = All.W9;
		if (Operators.CompareString(w9.StatusFN(), "", false) == 0 && w9.Initialization(strFN2))
		{
			bool result9 = w9.AddPayForm(strFN);
			All.ReplyRemember(typErrStr.FN, w9.StatusBarXML());
			w9.Finalization(strFN2);
			return result9;
		}
		w9 = null;
		All.Log.SaveTextToLog(typErrStr.FN, "AddPayForm", strFN, "Все слоты заняты транзакциями с сервером налоговой");
		All.ReplyRemember(typErrStr.FN, All.XMLRiplyErr(typErrStr.FN, "Все слоты заняты транзакциями с сервером налоговой"));
		return false;
	}

	public bool UpdatePayFrom(string strFN)
	{
		TypErrStr typErrStr = All.TestFN(strFN);
		if (typErrStr.errCode > 0)
		{
			All.ReplyRemember(typErrStr.FN, All.XMLRiplyErr(typErrStr.FN, typErrStr.errStr));
			All.Log.SaveTextToLog(typErrStr.FN, "UpdatePayFrom", strFN, typErrStr.errStr);
			return false;
		}
		string strFN2 = "<InputParameters><Parameters FN='" + typErrStr.FN + "'/></InputParameters>";
		WebCheck1.ClassFiscal w = All.W1;
		if (Operators.CompareString(w.StatusFN(), "", false) == 0 && w.Initialization(strFN2))
		{
			bool result = w.UpdatePayFrom(strFN);
			All.ReplyRemember(typErrStr.FN, w.StatusBarXML());
			w.Finalization(strFN2);
			return result;
		}
		w = null;
		WebCheck2.ClassFiscal w2 = All.W2;
		if (Operators.CompareString(w2.StatusFN(), "", false) == 0 && w2.Initialization(strFN2))
		{
			bool result2 = w2.UpdatePayFrom(strFN);
			All.ReplyRemember(typErrStr.FN, w2.StatusBarXML());
			w2.Finalization(strFN2);
			return result2;
		}
		w2 = null;
		WebCheck3.ClassFiscal w3 = All.W3;
		if (Operators.CompareString(w3.StatusFN(), "", false) == 0 && w3.Initialization(strFN2))
		{
			bool result3 = w3.UpdatePayFrom(strFN);
			All.ReplyRemember(typErrStr.FN, w3.StatusBarXML());
			w3.Finalization(strFN2);
			return result3;
		}
		w3 = null;
		WebCheck4.ClassFiscal w4 = All.W4;
		if (Operators.CompareString(w4.StatusFN(), "", false) == 0 && w4.Initialization(strFN2))
		{
			bool result4 = w4.UpdatePayFrom(strFN);
			All.ReplyRemember(typErrStr.FN, w4.StatusBarXML());
			w4.Finalization(strFN2);
			return result4;
		}
		w4 = null;
		WebCheck5.ClassFiscal w5 = All.W5;
		if (Operators.CompareString(w5.StatusFN(), "", false) == 0 && w5.Initialization(strFN2))
		{
			bool result5 = w5.UpdatePayFrom(strFN);
			All.ReplyRemember(typErrStr.FN, w5.StatusBarXML());
			w5.Finalization(strFN2);
			return result5;
		}
		w5 = null;
		WebCheck6.ClassFiscal w6 = All.W6;
		if (Operators.CompareString(w6.StatusFN(), "", false) == 0 && w6.Initialization(strFN2))
		{
			bool result6 = w6.UpdatePayFrom(strFN);
			All.ReplyRemember(typErrStr.FN, w6.StatusBarXML());
			w6.Finalization(strFN2);
			return result6;
		}
		w6 = null;
		WebCheck7.ClassFiscal w7 = All.W7;
		if (Operators.CompareString(w7.StatusFN(), "", false) == 0 && w7.Initialization(strFN2))
		{
			bool result7 = w7.UpdatePayFrom(strFN);
			All.ReplyRemember(typErrStr.FN, w7.StatusBarXML());
			w7.Finalization(strFN2);
			return result7;
		}
		w7 = null;
		WebCheck8.ClassFiscal w8 = All.W8;
		if (Operators.CompareString(w8.StatusFN(), "", false) == 0 && w8.Initialization(strFN2))
		{
			bool result8 = w8.UpdatePayFrom(strFN);
			All.ReplyRemember(typErrStr.FN, w8.StatusBarXML());
			w8.Finalization(strFN2);
			return result8;
		}
		w8 = null;
		WebCheck9.ClassFiscal w9 = All.W9;
		if (Operators.CompareString(w9.StatusFN(), "", false) == 0 && w9.Initialization(strFN2))
		{
			bool result9 = w9.UpdatePayFrom(strFN);
			All.ReplyRemember(typErrStr.FN, w9.StatusBarXML());
			w9.Finalization(strFN2);
			return result9;
		}
		w9 = null;
		WebCheck10.ClassFiscal w10 = All.W10;
		if (Operators.CompareString(w10.StatusFN(), "", false) == 0 && w10.Initialization(strFN2))
		{
			bool result10 = w10.UpdatePayFrom(strFN);
			All.ReplyRemember(typErrStr.FN, w10.StatusBarXML());
			w10.Finalization(strFN2);
			return result10;
		}
		w10 = null;
		WebCheck11.ClassFiscal w11 = All.W11;
		if (Operators.CompareString(w11.StatusFN(), "", false) == 0 && w11.Initialization(strFN2))
		{
			bool result11 = w11.UpdatePayFrom(strFN);
			All.ReplyRemember(typErrStr.FN, w11.StatusBarXML());
			w11.Finalization(strFN2);
			return result11;
		}
		w11 = null;
		WebCheck12.ClassFiscal w12 = All.W12;
		if (Operators.CompareString(w12.StatusFN(), "", false) == 0 && w12.Initialization(strFN2))
		{
			bool result12 = w12.UpdatePayFrom(strFN);
			All.ReplyRemember(typErrStr.FN, w12.StatusBarXML());
			w12.Finalization(strFN2);
			return result12;
		}
		w12 = null;
		WebCheck13.ClassFiscal w13 = All.W13;
		if (Operators.CompareString(w13.StatusFN(), "", false) == 0 && w13.Initialization(strFN2))
		{
			bool result13 = w13.UpdatePayFrom(strFN);
			All.ReplyRemember(typErrStr.FN, w13.StatusBarXML());
			w13.Finalization(strFN2);
			return result13;
		}
		w13 = null;
		WebCheck14.ClassFiscal w14 = All.W14;
		if (Operators.CompareString(w14.StatusFN(), "", false) == 0 && w14.Initialization(strFN2))
		{
			bool result14 = w14.UpdatePayFrom(strFN);
			All.ReplyRemember(typErrStr.FN, w14.StatusBarXML());
			w14.Finalization(strFN2);
			return result14;
		}
		w14 = null;
		WebCheck15.ClassFiscal w15 = All.W15;
		if (Operators.CompareString(w15.StatusFN(), "", false) == 0 && w15.Initialization(strFN2))
		{
			bool result15 = w15.UpdatePayFrom(strFN);
			All.ReplyRemember(typErrStr.FN, w15.StatusBarXML());
			w15.Finalization(strFN2);
			return result15;
		}
		w15 = null;
		WebCheck16.ClassFiscal w16 = All.W16;
		if (Operators.CompareString(w16.StatusFN(), "", false) == 0 && w16.Initialization(strFN2))
		{
			bool result16 = w16.UpdatePayFrom(strFN);
			All.ReplyRemember(typErrStr.FN, w16.StatusBarXML());
			w16.Finalization(strFN2);
			return result16;
		}
		w16 = null;
		WebCheck17.ClassFiscal w17 = All.W17;
		if (Operators.CompareString(w17.StatusFN(), "", false) == 0 && w17.Initialization(strFN2))
		{
			bool result17 = w17.UpdatePayFrom(strFN);
			All.ReplyRemember(typErrStr.FN, w17.StatusBarXML());
			w17.Finalization(strFN2);
			return result17;
		}
		w17 = null;
		WebCheck18.ClassFiscal w18 = All.W18;
		if (Operators.CompareString(w18.StatusFN(), "", false) == 0 && w18.Initialization(strFN2))
		{
			bool result18 = w18.UpdatePayFrom(strFN);
			All.ReplyRemember(typErrStr.FN, w18.StatusBarXML());
			w18.Finalization(strFN2);
			return result18;
		}
		w18 = null;
		WebCheck19.ClassFiscal w19 = All.W19;
		if (Operators.CompareString(w19.StatusFN(), "", false) == 0 && w19.Initialization(strFN2))
		{
			bool result19 = w19.UpdatePayFrom(strFN);
			All.ReplyRemember(typErrStr.FN, w19.StatusBarXML());
			w19.Finalization(strFN2);
			return result19;
		}
		w19 = null;
		WebCheck20.ClassFiscal w20 = All.W20;
		if (Operators.CompareString(w20.StatusFN(), "", false) == 0 && w20.Initialization(strFN2))
		{
			bool result20 = w20.UpdatePayFrom(strFN);
			All.ReplyRemember(typErrStr.FN, w20.StatusBarXML());
			w20.Finalization(strFN2);
			return result20;
		}
		w20 = null;
		WebCheck21.ClassFiscal w21 = All.W21;
		if (Operators.CompareString(w21.StatusFN(), "", false) == 0 && w21.Initialization(strFN2))
		{
			bool result21 = w21.UpdatePayFrom(strFN);
			All.ReplyRemember(typErrStr.FN, w21.StatusBarXML());
			w21.Finalization(strFN2);
			return result21;
		}
		w21 = null;
		All.Log.SaveTextToLog(typErrStr.FN, "UpdatePayFrom", strFN, "Все слоты заняты транзакциями с сервером налоговой");
		All.ReplyRemember(typErrStr.FN, All.XMLRiplyErr(typErrStr.FN, "Все слоты заняты транзакциями с сервером налоговой"));
		return false;
	}

	public bool AddPRRO(string strFN)
	{
		TypErrStr typErrStr = All.TestFN(strFN);
		if (typErrStr.FN.Length != 10)
		{
			All.ReplyRemember(typErrStr.FN, "Ошибка формата фискального номера.");
			All.Log.SaveTextToLog(typErrStr.FN, "AddPRRO", strFN, "Ошибка формата фискального номера.");
			return false;
		}
		if (!Versioned.IsNumeric((object)typErrStr.FN))
		{
			All.ReplyRemember(typErrStr.FN, "Ошибка формата фискального номера.");
			All.Log.SaveTextToLog(typErrStr.FN, "AddPRRO", strFN, "Ошибка формата фискального номера.");
			return false;
		}
		TypErrStr typErrStr2 = All.TestFNuniquely(typErrStr.FN);
		if (typErrStr2.errCode > 0)
		{
			All.ReplyRemember(typErrStr.FN, typErrStr2.errStr);
			All.Log.SaveTextToLog(typErrStr.FN, "AddPRRO", strFN, typErrStr2.errStr);
		}
		string strFN2 = "<InputParameters><Parameters FN='" + typErrStr.FN + "'/></InputParameters>";
		WebCheck1.ClassFiscal w = All.W1;
		if (Operators.CompareString(w.StatusFN(), "", false) == 0 && w.ServerSetGet.InitializationForAddPRRO(strFN2))
		{
			bool result = w.AddPRRO(strFN);
			All.ReplyRemember(typErrStr.FN, w.StatusBarXML());
			w.Finalization(strFN2);
			return result;
		}
		w = null;
		WebCheck2.ClassFiscal w2 = All.W2;
		if (Operators.CompareString(w2.StatusFN(), "", false) == 0 && w2.ServerSetGet.InitializationForAddPRRO(strFN2))
		{
			bool result2 = w2.AddPRRO(strFN);
			All.ReplyRemember(typErrStr.FN, w2.StatusBarXML());
			w2.Finalization(strFN2);
			return result2;
		}
		w2 = null;
		WebCheck3.ClassFiscal w3 = All.W3;
		if (Operators.CompareString(w3.StatusFN(), "", false) == 0 && w3.ServerSetGet.InitializationForAddPRRO(strFN2))
		{
			bool result3 = w3.AddPRRO(strFN);
			All.ReplyRemember(typErrStr.FN, w3.StatusBarXML());
			w3.Finalization(strFN2);
			return result3;
		}
		w3 = null;
		WebCheck4.ClassFiscal w4 = All.W4;
		if (Operators.CompareString(w4.StatusFN(), "", false) == 0 && w4.ServerSetGet.InitializationForAddPRRO(strFN2))
		{
			bool result4 = w4.AddPRRO(strFN);
			All.ReplyRemember(typErrStr.FN, w4.StatusBarXML());
			w4.Finalization(strFN2);
			return result4;
		}
		w4 = null;
		WebCheck5.ClassFiscal w5 = All.W5;
		if (Operators.CompareString(w5.StatusFN(), "", false) == 0 && w5.ServerSetGet.InitializationForAddPRRO(strFN2))
		{
			bool result5 = w5.AddPRRO(strFN);
			All.ReplyRemember(typErrStr.FN, w5.StatusBarXML());
			w5.Finalization(strFN2);
			return result5;
		}
		w5 = null;
		WebCheck6.ClassFiscal w6 = All.W6;
		if (Operators.CompareString(w6.StatusFN(), "", false) == 0 && w6.ServerSetGet.InitializationForAddPRRO(strFN2))
		{
			bool result6 = w6.AddPRRO(strFN);
			All.ReplyRemember(typErrStr.FN, w6.StatusBarXML());
			w6.Finalization(strFN2);
			return result6;
		}
		w6 = null;
		WebCheck7.ClassFiscal w7 = All.W7;
		if (Operators.CompareString(w7.StatusFN(), "", false) == 0 && w7.ServerSetGet.InitializationForAddPRRO(strFN2))
		{
			bool result7 = w7.AddPRRO(strFN);
			All.ReplyRemember(typErrStr.FN, w7.StatusBarXML());
			w7.Finalization(strFN2);
			return result7;
		}
		w7 = null;
		WebCheck8.ClassFiscal w8 = All.W8;
		if (Operators.CompareString(w8.StatusFN(), "", false) == 0 && w8.ServerSetGet.InitializationForAddPRRO(strFN2))
		{
			bool result8 = w8.AddPRRO(strFN);
			All.ReplyRemember(typErrStr.FN, w8.StatusBarXML());
			w8.Finalization(strFN2);
			return result8;
		}
		w8 = null;
		WebCheck9.ClassFiscal w9 = All.W9;
		if (Operators.CompareString(w9.StatusFN(), "", false) == 0 && w9.ServerSetGet.InitializationForAddPRRO(strFN2))
		{
			bool result9 = w9.AddPRRO(strFN);
			All.ReplyRemember(typErrStr.FN, w9.StatusBarXML());
			w9.Finalization(strFN2);
			return result9;
		}
		w9 = null;
		All.Log.SaveTextToLog(typErrStr.FN, "AddPRRO", strFN, "Все слоты заняты транзакциями с сервером налоговой");
		All.ReplyRemember(typErrStr.FN, All.XMLRiplyErr(typErrStr.FN, "Все слоты заняты транзакциями с сервером налоговой"));
		return false;
	}

	public bool AddOperator(string strFN)
	{
		TypErrStr typErrStr = All.TestFN(strFN);
		if (typErrStr.errCode > 0)
		{
			All.ReplyRemember(typErrStr.FN, All.XMLRiplyErr(typErrStr.FN, typErrStr.errStr));
			All.Log.SaveTextToLog(typErrStr.FN, "AddOperator", strFN, typErrStr.errStr);
			return false;
		}
		string strFN2 = "<InputParameters><Parameters FN='" + typErrStr.FN + "'/></InputParameters>";
		WebCheck1.ClassFiscal w = All.W1;
		if (Operators.CompareString(w.StatusFN(), "", false) == 0 && w.Initialization(strFN2))
		{
			bool result = w.AddOperator(strFN);
			All.ReplyRemember(typErrStr.FN, w.StatusBarXML());
			w.Finalization(strFN2);
			return result;
		}
		w = null;
		WebCheck2.ClassFiscal w2 = All.W2;
		if (Operators.CompareString(w2.StatusFN(), "", false) == 0 && w2.Initialization(strFN2))
		{
			bool result2 = w2.AddOperator(strFN);
			All.ReplyRemember(typErrStr.FN, w2.StatusBarXML());
			w2.Finalization(strFN2);
			return result2;
		}
		w2 = null;
		WebCheck3.ClassFiscal w3 = All.W3;
		if (Operators.CompareString(w3.StatusFN(), "", false) == 0 && w3.Initialization(strFN2))
		{
			bool result3 = w3.AddOperator(strFN);
			All.ReplyRemember(typErrStr.FN, w3.StatusBarXML());
			w3.Finalization(strFN2);
			return result3;
		}
		w3 = null;
		WebCheck4.ClassFiscal w4 = All.W4;
		if (Operators.CompareString(w4.StatusFN(), "", false) == 0 && w4.Initialization(strFN2))
		{
			bool result4 = w4.AddOperator(strFN);
			All.ReplyRemember(typErrStr.FN, w4.StatusBarXML());
			w4.Finalization(strFN2);
			return result4;
		}
		w4 = null;
		WebCheck5.ClassFiscal w5 = All.W5;
		if (Operators.CompareString(w5.StatusFN(), "", false) == 0 && w5.Initialization(strFN2))
		{
			bool result5 = w5.AddOperator(strFN);
			All.ReplyRemember(typErrStr.FN, w5.StatusBarXML());
			w5.Finalization(strFN2);
			return result5;
		}
		w5 = null;
		WebCheck6.ClassFiscal w6 = All.W6;
		if (Operators.CompareString(w6.StatusFN(), "", false) == 0 && w6.Initialization(strFN2))
		{
			bool result6 = w6.AddOperator(strFN);
			All.ReplyRemember(typErrStr.FN, w6.StatusBarXML());
			w6.Finalization(strFN2);
			return result6;
		}
		w6 = null;
		WebCheck7.ClassFiscal w7 = All.W7;
		if (Operators.CompareString(w7.StatusFN(), "", false) == 0 && w7.Initialization(strFN2))
		{
			bool result7 = w7.AddOperator(strFN);
			All.ReplyRemember(typErrStr.FN, w7.StatusBarXML());
			w7.Finalization(strFN2);
			return result7;
		}
		w7 = null;
		WebCheck8.ClassFiscal w8 = All.W8;
		if (Operators.CompareString(w8.StatusFN(), "", false) == 0 && w8.Initialization(strFN2))
		{
			bool result8 = w8.AddOperator(strFN);
			All.ReplyRemember(typErrStr.FN, w8.StatusBarXML());
			w8.Finalization(strFN2);
			return result8;
		}
		w8 = null;
		WebCheck9.ClassFiscal w9 = All.W9;
		if (Operators.CompareString(w9.StatusFN(), "", false) == 0 && w9.Initialization(strFN2))
		{
			bool result9 = w9.AddOperator(strFN);
			All.ReplyRemember(typErrStr.FN, w9.StatusBarXML());
			w9.Finalization(strFN2);
			return result9;
		}
		w9 = null;
		All.Log.SaveTextToLog(typErrStr.FN, "AddOperator", strFN, "Все слоты заняты транзакциями с сервером налоговой");
		All.ReplyRemember(typErrStr.FN, All.XMLRiplyErr(typErrStr.FN, "Все слоты заняты транзакциями с сервером налоговой"));
		return false;
	}

	public bool SendMessage(string strFN)
	{
		TypErrStr typErrStr = All.TestFN(strFN);
		if (typErrStr.errCode > 0)
		{
			All.ReplyRemember(typErrStr.FN, All.XMLRiplyErr(typErrStr.FN, typErrStr.errStr));
			All.Log.SaveTextToLog(typErrStr.FN, "SendMessage", strFN, typErrStr.errStr);
			return false;
		}
		string strFN2 = "<InputParameters><Parameters FN='" + typErrStr.FN + "'/></InputParameters>";
		WebCheck1.ClassFiscal w = All.W1;
		if (Operators.CompareString(w.StatusFN(), "", false) == 0 && w.Initialization(strFN2))
		{
			bool result = w.SendMessage(strFN);
			All.ReplyRemember(typErrStr.FN, w.StatusBarXML());
			w.Finalization(strFN2);
			return result;
		}
		w = null;
		WebCheck2.ClassFiscal w2 = All.W2;
		if (Operators.CompareString(w2.StatusFN(), "", false) == 0 && w2.Initialization(strFN2))
		{
			bool result2 = w2.SendMessage(strFN);
			All.ReplyRemember(typErrStr.FN, w2.StatusBarXML());
			w2.Finalization(strFN2);
			return result2;
		}
		w2 = null;
		WebCheck3.ClassFiscal w3 = All.W3;
		if (Operators.CompareString(w3.StatusFN(), "", false) == 0 && w3.Initialization(strFN2))
		{
			bool result3 = w3.SendMessage(strFN);
			All.ReplyRemember(typErrStr.FN, w3.StatusBarXML());
			w3.Finalization(strFN2);
			return result3;
		}
		w3 = null;
		WebCheck4.ClassFiscal w4 = All.W4;
		if (Operators.CompareString(w4.StatusFN(), "", false) == 0 && w4.Initialization(strFN2))
		{
			bool result4 = w4.SendMessage(strFN);
			All.ReplyRemember(typErrStr.FN, w4.StatusBarXML());
			w4.Finalization(strFN2);
			return result4;
		}
		w4 = null;
		WebCheck5.ClassFiscal w5 = All.W5;
		if (Operators.CompareString(w5.StatusFN(), "", false) == 0 && w5.Initialization(strFN2))
		{
			bool result5 = w5.SendMessage(strFN);
			All.ReplyRemember(typErrStr.FN, w5.StatusBarXML());
			w5.Finalization(strFN2);
			return result5;
		}
		w5 = null;
		WebCheck6.ClassFiscal w6 = All.W6;
		if (Operators.CompareString(w6.StatusFN(), "", false) == 0 && w6.Initialization(strFN2))
		{
			bool result6 = w6.SendMessage(strFN);
			All.ReplyRemember(typErrStr.FN, w6.StatusBarXML());
			w6.Finalization(strFN2);
			return result6;
		}
		w6 = null;
		WebCheck7.ClassFiscal w7 = All.W7;
		if (Operators.CompareString(w7.StatusFN(), "", false) == 0 && w7.Initialization(strFN2))
		{
			bool result7 = w7.SendMessage(strFN);
			All.ReplyRemember(typErrStr.FN, w7.StatusBarXML());
			w7.Finalization(strFN2);
			return result7;
		}
		w7 = null;
		WebCheck8.ClassFiscal w8 = All.W8;
		if (Operators.CompareString(w8.StatusFN(), "", false) == 0 && w8.Initialization(strFN2))
		{
			bool result8 = w8.SendMessage(strFN);
			All.ReplyRemember(typErrStr.FN, w8.StatusBarXML());
			w8.Finalization(strFN2);
			return result8;
		}
		w8 = null;
		WebCheck9.ClassFiscal w9 = All.W9;
		if (Operators.CompareString(w9.StatusFN(), "", false) == 0 && w9.Initialization(strFN2))
		{
			bool result9 = w9.SendMessage(strFN);
			All.ReplyRemember(typErrStr.FN, w9.StatusBarXML());
			w9.Finalization(strFN2);
			return result9;
		}
		w9 = null;
		WebCheck10.ClassFiscal w10 = All.W10;
		if (Operators.CompareString(w10.StatusFN(), "", false) == 0 && w10.Initialization(strFN2))
		{
			bool result10 = w10.SendMessage(strFN);
			All.ReplyRemember(typErrStr.FN, w10.StatusBarXML());
			w10.Finalization(strFN2);
			return result10;
		}
		w10 = null;
		WebCheck11.ClassFiscal w11 = All.W11;
		if (Operators.CompareString(w11.StatusFN(), "", false) == 0 && w11.Initialization(strFN2))
		{
			bool result11 = w11.SendMessage(strFN);
			All.ReplyRemember(typErrStr.FN, w11.StatusBarXML());
			w11.Finalization(strFN2);
			return result11;
		}
		w11 = null;
		WebCheck12.ClassFiscal w12 = All.W12;
		if (Operators.CompareString(w12.StatusFN(), "", false) == 0 && w12.Initialization(strFN2))
		{
			bool result12 = w12.SendMessage(strFN);
			All.ReplyRemember(typErrStr.FN, w12.StatusBarXML());
			w12.Finalization(strFN2);
			return result12;
		}
		w12 = null;
		WebCheck13.ClassFiscal w13 = All.W13;
		if (Operators.CompareString(w13.StatusFN(), "", false) == 0 && w13.Initialization(strFN2))
		{
			bool result13 = w13.SendMessage(strFN);
			All.ReplyRemember(typErrStr.FN, w13.StatusBarXML());
			w13.Finalization(strFN2);
			return result13;
		}
		w13 = null;
		WebCheck14.ClassFiscal w14 = All.W14;
		if (Operators.CompareString(w14.StatusFN(), "", false) == 0 && w14.Initialization(strFN2))
		{
			bool result14 = w14.SendMessage(strFN);
			All.ReplyRemember(typErrStr.FN, w14.StatusBarXML());
			w14.Finalization(strFN2);
			return result14;
		}
		w14 = null;
		WebCheck15.ClassFiscal w15 = All.W15;
		if (Operators.CompareString(w15.StatusFN(), "", false) == 0 && w15.Initialization(strFN2))
		{
			bool result15 = w15.SendMessage(strFN);
			All.ReplyRemember(typErrStr.FN, w15.StatusBarXML());
			w15.Finalization(strFN2);
			return result15;
		}
		w15 = null;
		WebCheck16.ClassFiscal w16 = All.W16;
		if (Operators.CompareString(w16.StatusFN(), "", false) == 0 && w16.Initialization(strFN2))
		{
			bool result16 = w16.SendMessage(strFN);
			All.ReplyRemember(typErrStr.FN, w16.StatusBarXML());
			w16.Finalization(strFN2);
			return result16;
		}
		w16 = null;
		WebCheck17.ClassFiscal w17 = All.W17;
		if (Operators.CompareString(w17.StatusFN(), "", false) == 0 && w17.Initialization(strFN2))
		{
			bool result17 = w17.SendMessage(strFN);
			All.ReplyRemember(typErrStr.FN, w17.StatusBarXML());
			w17.Finalization(strFN2);
			return result17;
		}
		w17 = null;
		WebCheck18.ClassFiscal w18 = All.W18;
		if (Operators.CompareString(w18.StatusFN(), "", false) == 0 && w18.Initialization(strFN2))
		{
			bool result18 = w18.SendMessage(strFN);
			All.ReplyRemember(typErrStr.FN, w18.StatusBarXML());
			w18.Finalization(strFN2);
			return result18;
		}
		w18 = null;
		WebCheck19.ClassFiscal w19 = All.W19;
		if (Operators.CompareString(w19.StatusFN(), "", false) == 0 && w19.Initialization(strFN2))
		{
			bool result19 = w19.SendMessage(strFN);
			All.ReplyRemember(typErrStr.FN, w19.StatusBarXML());
			w19.Finalization(strFN2);
			return result19;
		}
		w19 = null;
		WebCheck20.ClassFiscal w20 = All.W20;
		if (Operators.CompareString(w20.StatusFN(), "", false) == 0 && w20.Initialization(strFN2))
		{
			bool result20 = w20.SendMessage(strFN);
			All.ReplyRemember(typErrStr.FN, w20.StatusBarXML());
			w20.Finalization(strFN2);
			return result20;
		}
		w20 = null;
		WebCheck21.ClassFiscal w21 = All.W21;
		if (Operators.CompareString(w21.StatusFN(), "", false) == 0 && w21.Initialization(strFN2))
		{
			bool result21 = w21.SendMessage(strFN);
			All.ReplyRemember(typErrStr.FN, w21.StatusBarXML());
			w21.Finalization(strFN2);
			return result21;
		}
		w21 = null;
		WebCheck22.ClassFiscal w22 = All.W22;
		if (Operators.CompareString(w22.StatusFN(), "", false) == 0 && w22.Initialization(strFN2))
		{
			bool result22 = w22.SendMessage(strFN);
			All.ReplyRemember(typErrStr.FN, w22.StatusBarXML());
			w22.Finalization(strFN2);
			return result22;
		}
		w22 = null;
		WebCheck23.ClassFiscal w23 = All.W23;
		if (Operators.CompareString(w23.StatusFN(), "", false) == 0 && w23.Initialization(strFN2))
		{
			bool result23 = w23.SendMessage(strFN);
			All.ReplyRemember(typErrStr.FN, w23.StatusBarXML());
			w23.Finalization(strFN2);
			return result23;
		}
		w23 = null;
		WebCheck24.ClassFiscal w24 = All.W24;
		if (Operators.CompareString(w24.StatusFN(), "", false) == 0 && w24.Initialization(strFN2))
		{
			bool result24 = w24.SendMessage(strFN);
			All.ReplyRemember(typErrStr.FN, w24.StatusBarXML());
			w24.Finalization(strFN2);
			return result24;
		}
		w24 = null;
		WebCheck25.ClassFiscal w25 = All.W25;
		if (Operators.CompareString(w25.StatusFN(), "", false) == 0 && w25.Initialization(strFN2))
		{
			bool result25 = w25.SendMessage(strFN);
			All.ReplyRemember(typErrStr.FN, w25.StatusBarXML());
			w25.Finalization(strFN2);
			return result25;
		}
		w25 = null;
		WebCheck26.ClassFiscal w26 = All.W26;
		if (Operators.CompareString(w26.StatusFN(), "", false) == 0 && w26.Initialization(strFN2))
		{
			bool result26 = w26.SendMessage(strFN);
			All.ReplyRemember(typErrStr.FN, w26.StatusBarXML());
			w26.Finalization(strFN2);
			return result26;
		}
		w26 = null;
		WebCheck27.ClassFiscal w27 = All.W27;
		if (Operators.CompareString(w27.StatusFN(), "", false) == 0 && w27.Initialization(strFN2))
		{
			bool result27 = w27.SendMessage(strFN);
			All.ReplyRemember(typErrStr.FN, w27.StatusBarXML());
			w27.Finalization(strFN2);
			return result27;
		}
		w27 = null;
		All.Log.SaveTextToLog(typErrStr.FN, "SendMessage", strFN, "Все слоты заняты транзакциями с сервером налоговой");
		All.ReplyRemember(typErrStr.FN, All.XMLRiplyErr(typErrStr.FN, "Все слоты заняты транзакциями с сервером налоговой"));
		return false;
	}

	public bool GetCheckcloudurl(string strFN)
	{
		TypErrStr typErrStr = All.TestFN(strFN);
		if (typErrStr.errCode > 0)
		{
			All.ReplyRemember(typErrStr.FN, All.XMLRiplyErr(typErrStr.FN, typErrStr.errStr));
			All.Log.SaveTextToLog(typErrStr.FN, "GetCheckcloudurl", strFN, typErrStr.errStr);
			return false;
		}
		string strFN2 = "<InputParameters><Parameters FN='" + typErrStr.FN + "'/></InputParameters>";
		WebCheck1.ClassFiscal w = All.W1;
		if (Operators.CompareString(w.StatusFN(), "", false) == 0 && w.Initialization(strFN2))
		{
			bool checkcloudurl = w.GetCheckcloudurl(strFN);
			All.ReplyRemember(typErrStr.FN, w.StatusBarXML());
			w.Finalization(strFN2);
			return checkcloudurl;
		}
		w = null;
		WebCheck2.ClassFiscal w2 = All.W2;
		if (Operators.CompareString(w2.StatusFN(), "", false) == 0 && w2.Initialization(strFN2))
		{
			bool checkcloudurl2 = w2.GetCheckcloudurl(strFN);
			All.ReplyRemember(typErrStr.FN, w2.StatusBarXML());
			w2.Finalization(strFN2);
			return checkcloudurl2;
		}
		w2 = null;
		WebCheck3.ClassFiscal w3 = All.W3;
		if (Operators.CompareString(w3.StatusFN(), "", false) == 0 && w3.Initialization(strFN2))
		{
			bool checkcloudurl3 = w3.GetCheckcloudurl(strFN);
			All.ReplyRemember(typErrStr.FN, w3.StatusBarXML());
			w3.Finalization(strFN2);
			return checkcloudurl3;
		}
		w3 = null;
		WebCheck4.ClassFiscal w4 = All.W4;
		if (Operators.CompareString(w4.StatusFN(), "", false) == 0 && w4.Initialization(strFN2))
		{
			bool checkcloudurl4 = w4.GetCheckcloudurl(strFN);
			All.ReplyRemember(typErrStr.FN, w4.StatusBarXML());
			w4.Finalization(strFN2);
			return checkcloudurl4;
		}
		w4 = null;
		WebCheck5.ClassFiscal w5 = All.W5;
		if (Operators.CompareString(w5.StatusFN(), "", false) == 0 && w5.Initialization(strFN2))
		{
			bool checkcloudurl5 = w5.GetCheckcloudurl(strFN);
			All.ReplyRemember(typErrStr.FN, w5.StatusBarXML());
			w5.Finalization(strFN2);
			return checkcloudurl5;
		}
		w5 = null;
		WebCheck6.ClassFiscal w6 = All.W6;
		if (Operators.CompareString(w6.StatusFN(), "", false) == 0 && w6.Initialization(strFN2))
		{
			bool checkcloudurl6 = w6.GetCheckcloudurl(strFN);
			All.ReplyRemember(typErrStr.FN, w6.StatusBarXML());
			w6.Finalization(strFN2);
			return checkcloudurl6;
		}
		w6 = null;
		WebCheck7.ClassFiscal w7 = All.W7;
		if (Operators.CompareString(w7.StatusFN(), "", false) == 0 && w7.Initialization(strFN2))
		{
			bool checkcloudurl7 = w7.GetCheckcloudurl(strFN);
			All.ReplyRemember(typErrStr.FN, w7.StatusBarXML());
			w7.Finalization(strFN2);
			return checkcloudurl7;
		}
		w7 = null;
		WebCheck8.ClassFiscal w8 = All.W8;
		if (Operators.CompareString(w8.StatusFN(), "", false) == 0 && w8.Initialization(strFN2))
		{
			bool checkcloudurl8 = w8.GetCheckcloudurl(strFN);
			All.ReplyRemember(typErrStr.FN, w8.StatusBarXML());
			w8.Finalization(strFN2);
			return checkcloudurl8;
		}
		w8 = null;
		WebCheck9.ClassFiscal w9 = All.W9;
		if (Operators.CompareString(w9.StatusFN(), "", false) == 0 && w9.Initialization(strFN2))
		{
			bool checkcloudurl9 = w9.GetCheckcloudurl(strFN);
			All.ReplyRemember(typErrStr.FN, w9.StatusBarXML());
			w9.Finalization(strFN2);
			return checkcloudurl9;
		}
		w9 = null;
		WebCheck10.ClassFiscal w10 = All.W10;
		if (Operators.CompareString(w10.StatusFN(), "", false) == 0 && w10.Initialization(strFN2))
		{
			bool checkcloudurl10 = w10.GetCheckcloudurl(strFN);
			All.ReplyRemember(typErrStr.FN, w10.StatusBarXML());
			w10.Finalization(strFN2);
			return checkcloudurl10;
		}
		w10 = null;
		WebCheck11.ClassFiscal w11 = All.W11;
		if (Operators.CompareString(w11.StatusFN(), "", false) == 0 && w11.Initialization(strFN2))
		{
			bool checkcloudurl11 = w11.GetCheckcloudurl(strFN);
			All.ReplyRemember(typErrStr.FN, w11.StatusBarXML());
			w11.Finalization(strFN2);
			return checkcloudurl11;
		}
		w11 = null;
		WebCheck12.ClassFiscal w12 = All.W12;
		if (Operators.CompareString(w12.StatusFN(), "", false) == 0 && w12.Initialization(strFN2))
		{
			bool checkcloudurl12 = w12.GetCheckcloudurl(strFN);
			All.ReplyRemember(typErrStr.FN, w12.StatusBarXML());
			w12.Finalization(strFN2);
			return checkcloudurl12;
		}
		w12 = null;
		WebCheck13.ClassFiscal w13 = All.W13;
		if (Operators.CompareString(w13.StatusFN(), "", false) == 0 && w13.Initialization(strFN2))
		{
			bool checkcloudurl13 = w13.GetCheckcloudurl(strFN);
			All.ReplyRemember(typErrStr.FN, w13.StatusBarXML());
			w13.Finalization(strFN2);
			return checkcloudurl13;
		}
		w13 = null;
		WebCheck14.ClassFiscal w14 = All.W14;
		if (Operators.CompareString(w14.StatusFN(), "", false) == 0 && w14.Initialization(strFN2))
		{
			bool checkcloudurl14 = w14.GetCheckcloudurl(strFN);
			All.ReplyRemember(typErrStr.FN, w14.StatusBarXML());
			w14.Finalization(strFN2);
			return checkcloudurl14;
		}
		w14 = null;
		WebCheck15.ClassFiscal w15 = All.W15;
		if (Operators.CompareString(w15.StatusFN(), "", false) == 0 && w15.Initialization(strFN2))
		{
			bool checkcloudurl15 = w15.GetCheckcloudurl(strFN);
			All.ReplyRemember(typErrStr.FN, w15.StatusBarXML());
			w15.Finalization(strFN2);
			return checkcloudurl15;
		}
		w15 = null;
		WebCheck16.ClassFiscal w16 = All.W16;
		if (Operators.CompareString(w16.StatusFN(), "", false) == 0 && w16.Initialization(strFN2))
		{
			bool checkcloudurl16 = w16.GetCheckcloudurl(strFN);
			All.ReplyRemember(typErrStr.FN, w16.StatusBarXML());
			w16.Finalization(strFN2);
			return checkcloudurl16;
		}
		w16 = null;
		WebCheck17.ClassFiscal w17 = All.W17;
		if (Operators.CompareString(w17.StatusFN(), "", false) == 0 && w17.Initialization(strFN2))
		{
			bool checkcloudurl17 = w17.GetCheckcloudurl(strFN);
			All.ReplyRemember(typErrStr.FN, w17.StatusBarXML());
			w17.Finalization(strFN2);
			return checkcloudurl17;
		}
		w17 = null;
		WebCheck18.ClassFiscal w18 = All.W18;
		if (Operators.CompareString(w18.StatusFN(), "", false) == 0 && w18.Initialization(strFN2))
		{
			bool checkcloudurl18 = w18.GetCheckcloudurl(strFN);
			All.ReplyRemember(typErrStr.FN, w18.StatusBarXML());
			w18.Finalization(strFN2);
			return checkcloudurl18;
		}
		w18 = null;
		WebCheck19.ClassFiscal w19 = All.W19;
		if (Operators.CompareString(w19.StatusFN(), "", false) == 0 && w19.Initialization(strFN2))
		{
			bool checkcloudurl19 = w19.GetCheckcloudurl(strFN);
			All.ReplyRemember(typErrStr.FN, w19.StatusBarXML());
			w19.Finalization(strFN2);
			return checkcloudurl19;
		}
		w19 = null;
		WebCheck20.ClassFiscal w20 = All.W20;
		if (Operators.CompareString(w20.StatusFN(), "", false) == 0 && w20.Initialization(strFN2))
		{
			bool checkcloudurl20 = w20.GetCheckcloudurl(strFN);
			All.ReplyRemember(typErrStr.FN, w20.StatusBarXML());
			w20.Finalization(strFN2);
			return checkcloudurl20;
		}
		w20 = null;
		WebCheck21.ClassFiscal w21 = All.W21;
		if (Operators.CompareString(w21.StatusFN(), "", false) == 0 && w21.Initialization(strFN2))
		{
			bool checkcloudurl21 = w21.GetCheckcloudurl(strFN);
			All.ReplyRemember(typErrStr.FN, w21.StatusBarXML());
			w21.Finalization(strFN2);
			return checkcloudurl21;
		}
		w21 = null;
		WebCheck22.ClassFiscal w22 = All.W22;
		if (Operators.CompareString(w22.StatusFN(), "", false) == 0 && w22.Initialization(strFN2))
		{
			bool checkcloudurl22 = w22.GetCheckcloudurl(strFN);
			All.ReplyRemember(typErrStr.FN, w22.StatusBarXML());
			w22.Finalization(strFN2);
			return checkcloudurl22;
		}
		w22 = null;
		WebCheck23.ClassFiscal w23 = All.W23;
		if (Operators.CompareString(w23.StatusFN(), "", false) == 0 && w23.Initialization(strFN2))
		{
			bool checkcloudurl23 = w23.GetCheckcloudurl(strFN);
			All.ReplyRemember(typErrStr.FN, w23.StatusBarXML());
			w23.Finalization(strFN2);
			return checkcloudurl23;
		}
		w23 = null;
		WebCheck24.ClassFiscal w24 = All.W24;
		if (Operators.CompareString(w24.StatusFN(), "", false) == 0 && w24.Initialization(strFN2))
		{
			bool checkcloudurl24 = w24.GetCheckcloudurl(strFN);
			All.ReplyRemember(typErrStr.FN, w24.StatusBarXML());
			w24.Finalization(strFN2);
			return checkcloudurl24;
		}
		w24 = null;
		WebCheck25.ClassFiscal w25 = All.W25;
		if (Operators.CompareString(w25.StatusFN(), "", false) == 0 && w25.Initialization(strFN2))
		{
			bool checkcloudurl25 = w25.GetCheckcloudurl(strFN);
			All.ReplyRemember(typErrStr.FN, w25.StatusBarXML());
			w25.Finalization(strFN2);
			return checkcloudurl25;
		}
		w25 = null;
		WebCheck26.ClassFiscal w26 = All.W26;
		if (Operators.CompareString(w26.StatusFN(), "", false) == 0 && w26.Initialization(strFN2))
		{
			bool checkcloudurl26 = w26.GetCheckcloudurl(strFN);
			All.ReplyRemember(typErrStr.FN, w26.StatusBarXML());
			w26.Finalization(strFN2);
			return checkcloudurl26;
		}
		w26 = null;
		WebCheck27.ClassFiscal w27 = All.W27;
		if (Operators.CompareString(w27.StatusFN(), "", false) == 0 && w27.Initialization(strFN2))
		{
			bool checkcloudurl27 = w27.GetCheckcloudurl(strFN);
			All.ReplyRemember(typErrStr.FN, w27.StatusBarXML());
			w27.Finalization(strFN2);
			return checkcloudurl27;
		}
		w27 = null;
		All.Log.SaveTextToLog(typErrStr.FN, "GetCheckcloudurl", strFN, "Все слоты заняты транзакциями с сервером налоговой");
		All.ReplyRemember(typErrStr.FN, All.XMLRiplyErr(typErrStr.FN, "Все слоты заняты транзакциями с сервером налоговой"));
		return false;
	}

	public string StatusBarXML(string strFN)
	{
		TypErrStr typErrStr = All.TestFN(strFN, "", TransactionControl: false);
		if (typErrStr.errCode > 0)
		{
			All.Log.SaveTextToLog(typErrStr.FN, "StatusBarXML", strFN, typErrStr.errStr);
			return typErrStr.errStr;
		}
		TypReply typReply = All.ReplyRemember(typErrStr.FN);
		if (Operators.CompareString(typReply.ReplyPrt, "", false) == 0)
		{
			return typReply.ReplyErr;
		}
		return typReply.ReplyPrt;
	}

	public bool GetCurrentStatus(string strFN)
	{
		checked
		{
			All.gS++;
			TypErrStr typErrStr = All.TestFN(strFN);
			if (typErrStr.errCode > 0)
			{
				All.ReplyRemember(typErrStr.FN, All.XMLRiplyErr(typErrStr.FN, typErrStr.errStr));
				All.Log.SaveTextToLog(typErrStr.FN, "GetCurrentStatus", strFN, typErrStr.errStr);
				return false;
			}
			string strFN2 = "<InputParameters><Parameters FN='" + typErrStr.FN + "'/></InputParameters>";
			WebCheck1.ClassFiscal w = All.W1;
			if (Operators.CompareString(w.StatusFN(), "", false) == 0 && w.Initialization(strFN2))
			{
				bool currentStatus = w.GetCurrentStatus(strFN);
				All.ReplyRemember(typErrStr.FN, w.StatusBarXML());
				w.Finalization(strFN2);
				return currentStatus;
			}
			w = null;
			WebCheck2.ClassFiscal w2 = All.W2;
			if (Operators.CompareString(w2.StatusFN(), "", false) == 0 && w2.Initialization(strFN2))
			{
				bool currentStatus2 = w2.GetCurrentStatus(strFN);
				All.ReplyRemember(typErrStr.FN, w2.StatusBarXML());
				w2.Finalization(strFN2);
				return currentStatus2;
			}
			w2 = null;
			WebCheck3.ClassFiscal w3 = All.W3;
			if (Operators.CompareString(w3.StatusFN(), "", false) == 0 && w3.Initialization(strFN2))
			{
				bool currentStatus3 = w3.GetCurrentStatus(strFN);
				All.ReplyRemember(typErrStr.FN, w3.StatusBarXML());
				w3.Finalization(strFN2);
				return currentStatus3;
			}
			w3 = null;
			WebCheck4.ClassFiscal w4 = All.W4;
			if (Operators.CompareString(w4.StatusFN(), "", false) == 0 && w4.Initialization(strFN2))
			{
				bool currentStatus4 = w4.GetCurrentStatus(strFN);
				All.ReplyRemember(typErrStr.FN, w4.StatusBarXML());
				w4.Finalization(strFN2);
				return currentStatus4;
			}
			w4 = null;
			WebCheck5.ClassFiscal w5 = All.W5;
			if (Operators.CompareString(w5.StatusFN(), "", false) == 0 && w5.Initialization(strFN2))
			{
				bool currentStatus5 = w5.GetCurrentStatus(strFN);
				All.ReplyRemember(typErrStr.FN, w5.StatusBarXML());
				w5.Finalization(strFN2);
				return currentStatus5;
			}
			w5 = null;
			WebCheck6.ClassFiscal w6 = All.W6;
			if (Operators.CompareString(w6.StatusFN(), "", false) == 0 && w6.Initialization(strFN2))
			{
				bool currentStatus6 = w6.GetCurrentStatus(strFN);
				All.ReplyRemember(typErrStr.FN, w6.StatusBarXML());
				w6.Finalization(strFN2);
				return currentStatus6;
			}
			w6 = null;
			WebCheck7.ClassFiscal w7 = All.W7;
			if (Operators.CompareString(w7.StatusFN(), "", false) == 0 && w7.Initialization(strFN2))
			{
				bool currentStatus7 = w7.GetCurrentStatus(strFN);
				All.ReplyRemember(typErrStr.FN, w7.StatusBarXML());
				w7.Finalization(strFN2);
				return currentStatus7;
			}
			w7 = null;
			WebCheck8.ClassFiscal w8 = All.W8;
			if (Operators.CompareString(w8.StatusFN(), "", false) == 0 && w8.Initialization(strFN2))
			{
				bool currentStatus8 = w8.GetCurrentStatus(strFN);
				All.ReplyRemember(typErrStr.FN, w8.StatusBarXML());
				w8.Finalization(strFN2);
				return currentStatus8;
			}
			w8 = null;
			WebCheck9.ClassFiscal w9 = All.W9;
			if (Operators.CompareString(w9.StatusFN(), "", false) == 0 && w9.Initialization(strFN2))
			{
				bool currentStatus9 = w9.GetCurrentStatus(strFN);
				All.ReplyRemember(typErrStr.FN, w9.StatusBarXML());
				w9.Finalization(strFN2);
				return currentStatus9;
			}
			w9 = null;
			WebCheck10.ClassFiscal w10 = All.W10;
			if (Operators.CompareString(w10.StatusFN(), "", false) == 0 && w10.Initialization(strFN2))
			{
				bool currentStatus10 = w10.GetCurrentStatus(strFN);
				All.ReplyRemember(typErrStr.FN, w10.StatusBarXML());
				w10.Finalization(strFN2);
				return currentStatus10;
			}
			w10 = null;
			WebCheck11.ClassFiscal w11 = All.W11;
			if (Operators.CompareString(w11.StatusFN(), "", false) == 0 && w11.Initialization(strFN2))
			{
				bool currentStatus11 = w11.GetCurrentStatus(strFN);
				All.ReplyRemember(typErrStr.FN, w11.StatusBarXML());
				w11.Finalization(strFN2);
				return currentStatus11;
			}
			w11 = null;
			WebCheck12.ClassFiscal w12 = All.W12;
			if (Operators.CompareString(w12.StatusFN(), "", false) == 0 && w12.Initialization(strFN2))
			{
				bool currentStatus12 = w12.GetCurrentStatus(strFN);
				All.ReplyRemember(typErrStr.FN, w12.StatusBarXML());
				w12.Finalization(strFN2);
				return currentStatus12;
			}
			w12 = null;
			WebCheck13.ClassFiscal w13 = All.W13;
			if (Operators.CompareString(w13.StatusFN(), "", false) == 0 && w13.Initialization(strFN2))
			{
				bool currentStatus13 = w13.GetCurrentStatus(strFN);
				All.ReplyRemember(typErrStr.FN, w13.StatusBarXML());
				w13.Finalization(strFN2);
				return currentStatus13;
			}
			w13 = null;
			WebCheck14.ClassFiscal w14 = All.W14;
			if (Operators.CompareString(w14.StatusFN(), "", false) == 0 && w14.Initialization(strFN2))
			{
				bool currentStatus14 = w14.GetCurrentStatus(strFN);
				All.ReplyRemember(typErrStr.FN, w14.StatusBarXML());
				w14.Finalization(strFN2);
				return currentStatus14;
			}
			w14 = null;
			WebCheck15.ClassFiscal w15 = All.W15;
			if (Operators.CompareString(w15.StatusFN(), "", false) == 0 && w15.Initialization(strFN2))
			{
				bool currentStatus15 = w15.GetCurrentStatus(strFN);
				All.ReplyRemember(typErrStr.FN, w15.StatusBarXML());
				w15.Finalization(strFN2);
				return currentStatus15;
			}
			w15 = null;
			WebCheck16.ClassFiscal w16 = All.W16;
			if (Operators.CompareString(w16.StatusFN(), "", false) == 0 && w16.Initialization(strFN2))
			{
				bool currentStatus16 = w16.GetCurrentStatus(strFN);
				All.ReplyRemember(typErrStr.FN, w16.StatusBarXML());
				w16.Finalization(strFN2);
				return currentStatus16;
			}
			w16 = null;
			WebCheck17.ClassFiscal w17 = All.W17;
			if (Operators.CompareString(w17.StatusFN(), "", false) == 0 && w17.Initialization(strFN2))
			{
				bool currentStatus17 = w17.GetCurrentStatus(strFN);
				All.ReplyRemember(typErrStr.FN, w17.StatusBarXML());
				w17.Finalization(strFN2);
				return currentStatus17;
			}
			w17 = null;
			WebCheck18.ClassFiscal w18 = All.W18;
			if (Operators.CompareString(w18.StatusFN(), "", false) == 0 && w18.Initialization(strFN2))
			{
				bool currentStatus18 = w18.GetCurrentStatus(strFN);
				All.ReplyRemember(typErrStr.FN, w18.StatusBarXML());
				w18.Finalization(strFN2);
				return currentStatus18;
			}
			w18 = null;
			WebCheck19.ClassFiscal w19 = All.W19;
			if (Operators.CompareString(w19.StatusFN(), "", false) == 0 && w19.Initialization(strFN2))
			{
				bool currentStatus19 = w19.GetCurrentStatus(strFN);
				All.ReplyRemember(typErrStr.FN, w19.StatusBarXML());
				w19.Finalization(strFN2);
				return currentStatus19;
			}
			w19 = null;
			WebCheck20.ClassFiscal w20 = All.W20;
			if (Operators.CompareString(w20.StatusFN(), "", false) == 0 && w20.Initialization(strFN2))
			{
				bool currentStatus20 = w20.GetCurrentStatus(strFN);
				All.ReplyRemember(typErrStr.FN, w20.StatusBarXML());
				w20.Finalization(strFN2);
				return currentStatus20;
			}
			w20 = null;
			WebCheck21.ClassFiscal w21 = All.W21;
			if (Operators.CompareString(w21.StatusFN(), "", false) == 0 && w21.Initialization(strFN2))
			{
				bool currentStatus21 = w21.GetCurrentStatus(strFN);
				All.ReplyRemember(typErrStr.FN, w21.StatusBarXML());
				w21.Finalization(strFN2);
				return currentStatus21;
			}
			w21 = null;
			WebCheck22.ClassFiscal w22 = All.W22;
			if (Operators.CompareString(w22.StatusFN(), "", false) == 0 && w22.Initialization(strFN2))
			{
				bool currentStatus22 = w22.GetCurrentStatus(strFN);
				All.ReplyRemember(typErrStr.FN, w22.StatusBarXML());
				w22.Finalization(strFN2);
				return currentStatus22;
			}
			w22 = null;
			WebCheck23.ClassFiscal w23 = All.W23;
			if (Operators.CompareString(w23.StatusFN(), "", false) == 0 && w23.Initialization(strFN2))
			{
				bool currentStatus23 = w23.GetCurrentStatus(strFN);
				All.ReplyRemember(typErrStr.FN, w23.StatusBarXML());
				w23.Finalization(strFN2);
				return currentStatus23;
			}
			w23 = null;
			WebCheck24.ClassFiscal w24 = All.W24;
			if (Operators.CompareString(w24.StatusFN(), "", false) == 0 && w24.Initialization(strFN2))
			{
				bool currentStatus24 = w24.GetCurrentStatus(strFN);
				All.ReplyRemember(typErrStr.FN, w24.StatusBarXML());
				w24.Finalization(strFN2);
				return currentStatus24;
			}
			w24 = null;
			WebCheck25.ClassFiscal w25 = All.W25;
			if (Operators.CompareString(w25.StatusFN(), "", false) == 0 && w25.Initialization(strFN2))
			{
				bool currentStatus25 = w25.GetCurrentStatus(strFN);
				All.ReplyRemember(typErrStr.FN, w25.StatusBarXML());
				w25.Finalization(strFN2);
				return currentStatus25;
			}
			w25 = null;
			WebCheck26.ClassFiscal w26 = All.W26;
			if (Operators.CompareString(w26.StatusFN(), "", false) == 0 && w26.Initialization(strFN2))
			{
				bool currentStatus26 = w26.GetCurrentStatus(strFN);
				All.ReplyRemember(typErrStr.FN, w26.StatusBarXML());
				w26.Finalization(strFN2);
				return currentStatus26;
			}
			w26 = null;
			WebCheck27.ClassFiscal w27 = All.W27;
			if (Operators.CompareString(w27.StatusFN(), "", false) == 0 && w27.Initialization(strFN2))
			{
				bool currentStatus27 = w27.GetCurrentStatus(strFN);
				All.ReplyRemember(typErrStr.FN, w27.StatusBarXML());
				w27.Finalization(strFN2);
				return currentStatus27;
			}
			w27 = null;
			WebCheck28.ClassFiscal w28 = All.W28;
			if (Operators.CompareString(w28.StatusFN(), "", false) == 0 && w28.Initialization(strFN2))
			{
				bool currentStatus28 = w28.GetCurrentStatus(strFN);
				All.ReplyRemember(typErrStr.FN, w28.StatusBarXML());
				w28.Finalization(strFN2);
				return currentStatus28;
			}
			w28 = null;
			WebCheck29.ClassFiscal w29 = All.W29;
			if (Operators.CompareString(w29.StatusFN(), "", false) == 0 && w29.Initialization(strFN2))
			{
				bool currentStatus29 = w29.GetCurrentStatus(strFN);
				All.ReplyRemember(typErrStr.FN, w29.StatusBarXML());
				w29.Finalization(strFN2);
				return currentStatus29;
			}
			w29 = null;
			WebCheck30.ClassFiscal w30 = All.W30;
			if (Operators.CompareString(w30.StatusFN(), "", false) == 0 && w30.Initialization(strFN2))
			{
				bool currentStatus30 = w30.GetCurrentStatus(strFN);
				All.ReplyRemember(typErrStr.FN, w30.StatusBarXML());
				w30.Finalization(strFN2);
				return currentStatus30;
			}
			w30 = null;
			All.gF++;
			string text = All.gS + "/" + All.gF;
			All.Log.SaveTextToLog(typErrStr.FN, "GetCurrentStatus " + text, strFN, "Все слоты заняты транзакциями с сервером налоговой");
			All.ReplyRemember(typErrStr.FN, All.XMLRiplyErr(typErrStr.FN, "Все слоты заняты транзакциями с сервером налоговой"));
			return false;
		}
	}

	public bool GetSetingsRRO(string strFN)
	{
		TypErrStr typErrStr = All.TestFN(strFN);
		if (typErrStr.errCode > 0)
		{
			All.ReplyRemember(typErrStr.FN, All.XMLRiplyErr(typErrStr.FN, typErrStr.errStr));
			All.Log.SaveTextToLog(typErrStr.FN, "GetSetingsRRO", strFN, typErrStr.errStr);
			return false;
		}
		string strFN2 = "<InputParameters><Parameters FN='" + typErrStr.FN + "'/></InputParameters>";
		WebCheck1.ClassFiscal w = All.W1;
		if (Operators.CompareString(w.StatusFN(), "", false) == 0 && w.Initialization(strFN2))
		{
			bool setingsRRO = w.GetSetingsRRO(strFN);
			All.ReplyRemember(typErrStr.FN, w.StatusBarXML());
			w.Finalization(strFN2);
			return setingsRRO;
		}
		w = null;
		WebCheck2.ClassFiscal w2 = All.W2;
		if (Operators.CompareString(w2.StatusFN(), "", false) == 0 && w2.Initialization(strFN2))
		{
			bool setingsRRO2 = w2.GetSetingsRRO(strFN);
			All.ReplyRemember(typErrStr.FN, w2.StatusBarXML());
			w2.Finalization(strFN2);
			return setingsRRO2;
		}
		w2 = null;
		WebCheck3.ClassFiscal w3 = All.W3;
		if (Operators.CompareString(w3.StatusFN(), "", false) == 0 && w3.Initialization(strFN2))
		{
			bool setingsRRO3 = w3.GetSetingsRRO(strFN);
			All.ReplyRemember(typErrStr.FN, w3.StatusBarXML());
			w3.Finalization(strFN2);
			return setingsRRO3;
		}
		w3 = null;
		WebCheck4.ClassFiscal w4 = All.W4;
		if (Operators.CompareString(w4.StatusFN(), "", false) == 0 && w4.Initialization(strFN2))
		{
			bool setingsRRO4 = w4.GetSetingsRRO(strFN);
			All.ReplyRemember(typErrStr.FN, w4.StatusBarXML());
			w4.Finalization(strFN2);
			return setingsRRO4;
		}
		w4 = null;
		WebCheck5.ClassFiscal w5 = All.W5;
		if (Operators.CompareString(w5.StatusFN(), "", false) == 0 && w5.Initialization(strFN2))
		{
			bool setingsRRO5 = w5.GetSetingsRRO(strFN);
			All.ReplyRemember(typErrStr.FN, w5.StatusBarXML());
			w5.Finalization(strFN2);
			return setingsRRO5;
		}
		w5 = null;
		WebCheck6.ClassFiscal w6 = All.W6;
		if (Operators.CompareString(w6.StatusFN(), "", false) == 0 && w6.Initialization(strFN2))
		{
			bool setingsRRO6 = w6.GetSetingsRRO(strFN);
			All.ReplyRemember(typErrStr.FN, w6.StatusBarXML());
			w6.Finalization(strFN2);
			return setingsRRO6;
		}
		w6 = null;
		WebCheck7.ClassFiscal w7 = All.W7;
		if (Operators.CompareString(w7.StatusFN(), "", false) == 0 && w7.Initialization(strFN2))
		{
			bool setingsRRO7 = w7.GetSetingsRRO(strFN);
			All.ReplyRemember(typErrStr.FN, w7.StatusBarXML());
			w7.Finalization(strFN2);
			return setingsRRO7;
		}
		w7 = null;
		WebCheck8.ClassFiscal w8 = All.W8;
		if (Operators.CompareString(w8.StatusFN(), "", false) == 0 && w8.Initialization(strFN2))
		{
			bool setingsRRO8 = w8.GetSetingsRRO(strFN);
			All.ReplyRemember(typErrStr.FN, w8.StatusBarXML());
			w8.Finalization(strFN2);
			return setingsRRO8;
		}
		w8 = null;
		WebCheck9.ClassFiscal w9 = All.W9;
		if (Operators.CompareString(w9.StatusFN(), "", false) == 0 && w9.Initialization(strFN2))
		{
			bool setingsRRO9 = w9.GetSetingsRRO(strFN);
			All.ReplyRemember(typErrStr.FN, w9.StatusBarXML());
			w9.Finalization(strFN2);
			return setingsRRO9;
		}
		w9 = null;
		WebCheck10.ClassFiscal w10 = All.W10;
		if (Operators.CompareString(w10.StatusFN(), "", false) == 0 && w10.Initialization(strFN2))
		{
			bool setingsRRO10 = w10.GetSetingsRRO(strFN);
			All.ReplyRemember(typErrStr.FN, w10.StatusBarXML());
			w10.Finalization(strFN2);
			return setingsRRO10;
		}
		w10 = null;
		WebCheck11.ClassFiscal w11 = All.W11;
		if (Operators.CompareString(w11.StatusFN(), "", false) == 0 && w11.Initialization(strFN2))
		{
			bool setingsRRO11 = w11.GetSetingsRRO(strFN);
			All.ReplyRemember(typErrStr.FN, w11.StatusBarXML());
			w11.Finalization(strFN2);
			return setingsRRO11;
		}
		w11 = null;
		WebCheck12.ClassFiscal w12 = All.W12;
		if (Operators.CompareString(w12.StatusFN(), "", false) == 0 && w12.Initialization(strFN2))
		{
			bool setingsRRO12 = w12.GetSetingsRRO(strFN);
			All.ReplyRemember(typErrStr.FN, w12.StatusBarXML());
			w12.Finalization(strFN2);
			return setingsRRO12;
		}
		w12 = null;
		WebCheck13.ClassFiscal w13 = All.W13;
		if (Operators.CompareString(w13.StatusFN(), "", false) == 0 && w13.Initialization(strFN2))
		{
			bool setingsRRO13 = w13.GetSetingsRRO(strFN);
			All.ReplyRemember(typErrStr.FN, w13.StatusBarXML());
			w13.Finalization(strFN2);
			return setingsRRO13;
		}
		w13 = null;
		WebCheck14.ClassFiscal w14 = All.W14;
		if (Operators.CompareString(w14.StatusFN(), "", false) == 0 && w14.Initialization(strFN2))
		{
			bool setingsRRO14 = w14.GetSetingsRRO(strFN);
			All.ReplyRemember(typErrStr.FN, w14.StatusBarXML());
			w14.Finalization(strFN2);
			return setingsRRO14;
		}
		w14 = null;
		WebCheck15.ClassFiscal w15 = All.W15;
		if (Operators.CompareString(w15.StatusFN(), "", false) == 0 && w15.Initialization(strFN2))
		{
			bool setingsRRO15 = w15.GetSetingsRRO(strFN);
			All.ReplyRemember(typErrStr.FN, w15.StatusBarXML());
			w15.Finalization(strFN2);
			return setingsRRO15;
		}
		w15 = null;
		WebCheck16.ClassFiscal w16 = All.W16;
		if (Operators.CompareString(w16.StatusFN(), "", false) == 0 && w16.Initialization(strFN2))
		{
			bool setingsRRO16 = w16.GetSetingsRRO(strFN);
			All.ReplyRemember(typErrStr.FN, w16.StatusBarXML());
			w16.Finalization(strFN2);
			return setingsRRO16;
		}
		w16 = null;
		WebCheck17.ClassFiscal w17 = All.W17;
		if (Operators.CompareString(w17.StatusFN(), "", false) == 0 && w17.Initialization(strFN2))
		{
			bool setingsRRO17 = w17.GetSetingsRRO(strFN);
			All.ReplyRemember(typErrStr.FN, w17.StatusBarXML());
			w17.Finalization(strFN2);
			return setingsRRO17;
		}
		w17 = null;
		WebCheck18.ClassFiscal w18 = All.W18;
		if (Operators.CompareString(w18.StatusFN(), "", false) == 0 && w18.Initialization(strFN2))
		{
			bool setingsRRO18 = w18.GetSetingsRRO(strFN);
			All.ReplyRemember(typErrStr.FN, w18.StatusBarXML());
			w18.Finalization(strFN2);
			return setingsRRO18;
		}
		w18 = null;
		All.Log.SaveTextToLog(typErrStr.FN, "GetSetingsRRO", strFN, "Все слоты заняты транзакциями с сервером налоговой");
		All.ReplyRemember(typErrStr.FN, All.XMLRiplyErr(typErrStr.FN, "Все слоты заняты транзакциями с сервером налоговой"));
		return false;
	}

	public bool GetCheck(string strFN)
	{
		TypErrStr typErrStr = All.TestFN(strFN);
		if (typErrStr.errCode > 0)
		{
			All.ReplyRemember(typErrStr.FN, All.XMLRiplyErr(typErrStr.FN, typErrStr.errStr));
			All.Log.SaveTextToLog(typErrStr.FN, "GetCheck", strFN, typErrStr.errStr);
			return false;
		}
		TypErrStr parametrToString = All.GetParametrToString(strFN, "TaxNum", "InputParameters/Parameters", RegUpLow: true);
		if (parametrToString.errCode > 0)
		{
			parametrToString = All.GetParametrToString(strFN, "taxnum", "InputParameters/Parameters", RegUpLow: true);
		}
		if (parametrToString.errCode > 0)
		{
			parametrToString = All.GetParametrToString(strFN, "Taxnum", "InputParameters/Parameters", RegUpLow: true);
		}
		if (parametrToString.errCode > 0)
		{
			parametrToString = All.GetParametrToString(strFN, "TAXNUM", "InputParameters/Parameters", RegUpLow: true);
		}
		if (parametrToString.errCode > 0)
		{
			parametrToString.ReturnStr = "";
		}
		TypErrStr parametrToString2 = All.GetParametrToString(strFN, "type");
		if (parametrToString2.errCode > 0)
		{
			parametrToString2.ReturnStr = "0";
		}
		int num = 0;
		if (Versioned.IsNumeric((object)parametrToString2.ReturnStr))
		{
			num = Conversions.ToInteger(parametrToString2.ReturnStr);
			if (num < 0)
			{
				num = 0;
			}
			if (num > 2)
			{
				num = 2;
			}
		}
		string strFN2 = "<InputParameters><Parameters FN='" + typErrStr.FN + "'/></InputParameters>";
		WebCheck1.ClassFiscal w = All.W1;
		if (Operators.CompareString(w.StatusFN(), "", false) == 0 && w.Initialization(strFN2))
		{
			string text = "";
			if (num == 0)
			{
				text = w.CheckLine.CheckXML(parametrToString.ReturnStr);
			}
			string text2 = "";
			if (num > 0)
			{
				string strFN3 = "<InputParameters><Parameters TaxNum='" + parametrToString.ReturnStr + "' Type='" + num + "' FN='" + typErrStr.FN + "'/></InputParameters>";
				if (w.GetCheckByFiscalNumber(strFN3))
				{
					text2 = w.StatusBarXML();
					text = "";
				}
			}
			if ((Operators.CompareString(text2.Trim(), "", false) == 0) & (Operators.CompareString(text.Trim(), "", false) == 0))
			{
				All.Log.SaveTextToLog(typErrStr.FN, "GetCheck", strFN, parametrToString.ReturnStr + " - чек не найден");
				All.ReplyRemember(typErrStr.FN, All.XMLRiplyErr(typErrStr.FN, parametrToString.ReturnStr + " - чек на найден"));
				w.Finalization(strFN2);
				return false;
			}
			if (All.ReplyRemember(typErrStr.FN, text2, text))
			{
				w.Finalization(strFN2);
				return true;
			}
			w.Finalization(strFN2);
			return false;
		}
		w = null;
		WebCheck2.ClassFiscal w2 = All.W2;
		if (Operators.CompareString(w2.StatusFN(), "", false) == 0 && w2.Initialization(strFN2))
		{
			string text3 = "";
			if (num == 0)
			{
				text3 = w2.CheckLine.CheckXML(parametrToString.ReturnStr);
			}
			string text4 = "";
			if (num > 0)
			{
				string strFN4 = "<InputParameters><Parameters TaxNum='" + parametrToString.ReturnStr + "' Type='" + num + "' FN='" + typErrStr.FN + "'/></InputParameters>";
				if (w2.GetCheckByFiscalNumber(strFN4))
				{
					text4 = w2.StatusBarXML();
					text3 = "";
				}
			}
			if ((Operators.CompareString(text4.Trim(), "", false) == 0) & (Operators.CompareString(text3.Trim(), "", false) == 0))
			{
				All.Log.SaveTextToLog(typErrStr.FN, "GetCheck", strFN, parametrToString.ReturnStr + " - чек не найден");
				All.ReplyRemember(typErrStr.FN, All.XMLRiplyErr(typErrStr.FN, parametrToString.ReturnStr + " - чек на найден"));
				w2.Finalization(strFN2);
				return false;
			}
			if (All.ReplyRemember(typErrStr.FN, text4, text3))
			{
				w2.Finalization(strFN2);
				return true;
			}
			w2.Finalization(strFN2);
			return false;
		}
		w2 = null;
		WebCheck3.ClassFiscal w3 = All.W3;
		if (Operators.CompareString(w3.StatusFN(), "", false) == 0 && w3.Initialization(strFN2))
		{
			string text5 = "";
			if (num == 0)
			{
				text5 = w3.CheckLine.CheckXML(parametrToString.ReturnStr);
			}
			string text6 = "";
			if (num > 0)
			{
				string strFN5 = "<InputParameters><Parameters TaxNum='" + parametrToString.ReturnStr + "' Type='" + num + "' FN='" + typErrStr.FN + "'/></InputParameters>";
				if (w3.GetCheckByFiscalNumber(strFN5))
				{
					text6 = w3.StatusBarXML();
					text5 = "";
				}
			}
			if ((Operators.CompareString(text6.Trim(), "", false) == 0) & (Operators.CompareString(text5.Trim(), "", false) == 0))
			{
				All.Log.SaveTextToLog(typErrStr.FN, "GetCheck", strFN, parametrToString.ReturnStr + " - чек не найден");
				All.ReplyRemember(typErrStr.FN, All.XMLRiplyErr(typErrStr.FN, parametrToString.ReturnStr + " - чек на найден"));
				w3.Finalization(strFN2);
				return false;
			}
			if (All.ReplyRemember(typErrStr.FN, text6, text5))
			{
				w3.Finalization(strFN2);
				return true;
			}
			w3.Finalization(strFN2);
			return false;
		}
		w3 = null;
		WebCheck4.ClassFiscal w4 = All.W4;
		if (Operators.CompareString(w4.StatusFN(), "", false) == 0 && w4.Initialization(strFN2))
		{
			string text7 = "";
			if (num == 0)
			{
				text7 = w4.CheckLine.CheckXML(parametrToString.ReturnStr);
			}
			string text8 = "";
			if (num > 0)
			{
				string strFN6 = "<InputParameters><Parameters TaxNum='" + parametrToString.ReturnStr + "' Type='" + num + "' FN='" + typErrStr.FN + "'/></InputParameters>";
				if (w4.GetCheckByFiscalNumber(strFN6))
				{
					text8 = w4.StatusBarXML();
					text7 = "";
				}
			}
			if ((Operators.CompareString(text8.Trim(), "", false) == 0) & (Operators.CompareString(text7.Trim(), "", false) == 0))
			{
				All.Log.SaveTextToLog(typErrStr.FN, "GetCheck", strFN, parametrToString.ReturnStr + " - чек не найден");
				All.ReplyRemember(typErrStr.FN, All.XMLRiplyErr(typErrStr.FN, parametrToString.ReturnStr + " - чек на найден"));
				w4.Finalization(strFN2);
				return false;
			}
			if (All.ReplyRemember(typErrStr.FN, text8, text7))
			{
				w4.Finalization(strFN2);
				return true;
			}
			w4.Finalization(strFN2);
			return false;
		}
		w4 = null;
		WebCheck5.ClassFiscal w5 = All.W5;
		if (Operators.CompareString(w5.StatusFN(), "", false) == 0 && w5.Initialization(strFN2))
		{
			string text9 = "";
			if (num == 0)
			{
				text9 = w5.CheckLine.CheckXML(parametrToString.ReturnStr);
			}
			string text10 = "";
			if (num > 0)
			{
				string strFN7 = "<InputParameters><Parameters TaxNum='" + parametrToString.ReturnStr + "' Type='" + num + "' FN='" + typErrStr.FN + "'/></InputParameters>";
				if (w5.GetCheckByFiscalNumber(strFN7))
				{
					text10 = w5.StatusBarXML();
					text9 = "";
				}
			}
			if ((Operators.CompareString(text10.Trim(), "", false) == 0) & (Operators.CompareString(text9.Trim(), "", false) == 0))
			{
				All.Log.SaveTextToLog(typErrStr.FN, "GetCheck", strFN, parametrToString.ReturnStr + " - чек не найден");
				All.ReplyRemember(typErrStr.FN, All.XMLRiplyErr(typErrStr.FN, parametrToString.ReturnStr + " - чек на найден"));
				w5.Finalization(strFN2);
				return false;
			}
			if (All.ReplyRemember(typErrStr.FN, text10, text9))
			{
				w5.Finalization(strFN2);
				return true;
			}
			w5.Finalization(strFN2);
			return false;
		}
		w5 = null;
		WebCheck6.ClassFiscal w6 = All.W6;
		if (Operators.CompareString(w6.StatusFN(), "", false) == 0 && w6.Initialization(strFN2))
		{
			string text11 = "";
			if (num == 0)
			{
				text11 = w6.CheckLine.CheckXML(parametrToString.ReturnStr);
			}
			string text12 = "";
			if (num > 0)
			{
				string strFN8 = "<InputParameters><Parameters TaxNum='" + parametrToString.ReturnStr + "' Type='" + num + "' FN='" + typErrStr.FN + "'/></InputParameters>";
				if (w6.GetCheckByFiscalNumber(strFN8))
				{
					text12 = w6.StatusBarXML();
					text11 = "";
				}
			}
			if ((Operators.CompareString(text12.Trim(), "", false) == 0) & (Operators.CompareString(text11.Trim(), "", false) == 0))
			{
				All.Log.SaveTextToLog(typErrStr.FN, "GetCheck", strFN, parametrToString.ReturnStr + " - чек не найден");
				All.ReplyRemember(typErrStr.FN, All.XMLRiplyErr(typErrStr.FN, parametrToString.ReturnStr + " - чек на найден"));
				w6.Finalization(strFN2);
				return false;
			}
			if (All.ReplyRemember(typErrStr.FN, text12, text11))
			{
				w6.Finalization(strFN2);
				return true;
			}
			w6.Finalization(strFN2);
			return false;
		}
		w6 = null;
		WebCheck7.ClassFiscal w7 = All.W7;
		if (Operators.CompareString(w7.StatusFN(), "", false) == 0 && w7.Initialization(strFN2))
		{
			string text13 = "";
			if (num == 0)
			{
				text13 = w7.CheckLine.CheckXML(parametrToString.ReturnStr);
			}
			string text14 = "";
			if (num > 0)
			{
				string strFN9 = "<InputParameters><Parameters TaxNum='" + parametrToString.ReturnStr + "' Type='" + num + "' FN='" + typErrStr.FN + "'/></InputParameters>";
				if (w7.GetCheckByFiscalNumber(strFN9))
				{
					text14 = w7.StatusBarXML();
					text13 = "";
				}
			}
			if ((Operators.CompareString(text14.Trim(), "", false) == 0) & (Operators.CompareString(text13.Trim(), "", false) == 0))
			{
				All.Log.SaveTextToLog(typErrStr.FN, "GetCheck", strFN, parametrToString.ReturnStr + " - чек не найден");
				All.ReplyRemember(typErrStr.FN, All.XMLRiplyErr(typErrStr.FN, parametrToString.ReturnStr + " - чек на найден"));
				w7.Finalization(strFN2);
				return false;
			}
			if (All.ReplyRemember(typErrStr.FN, text14, text13))
			{
				w7.Finalization(strFN2);
				return true;
			}
			w7.Finalization(strFN2);
			return false;
		}
		w7 = null;
		WebCheck8.ClassFiscal w8 = All.W8;
		if (Operators.CompareString(w8.StatusFN(), "", false) == 0 && w8.Initialization(strFN2))
		{
			string text15 = "";
			if (num == 0)
			{
				text15 = w8.CheckLine.CheckXML(parametrToString.ReturnStr);
			}
			string text16 = "";
			if (num > 0)
			{
				string strFN10 = "<InputParameters><Parameters TaxNum='" + parametrToString.ReturnStr + "' Type='" + num + "' FN='" + typErrStr.FN + "'/></InputParameters>";
				if (w8.GetCheckByFiscalNumber(strFN10))
				{
					text16 = w8.StatusBarXML();
					text15 = "";
				}
			}
			if ((Operators.CompareString(text16.Trim(), "", false) == 0) & (Operators.CompareString(text15.Trim(), "", false) == 0))
			{
				All.Log.SaveTextToLog(typErrStr.FN, "GetCheck", strFN, parametrToString.ReturnStr + " - чек не найден");
				All.ReplyRemember(typErrStr.FN, All.XMLRiplyErr(typErrStr.FN, parametrToString.ReturnStr + " - чек на найден"));
				w8.Finalization(strFN2);
				return false;
			}
			if (All.ReplyRemember(typErrStr.FN, text16, text15))
			{
				w8.Finalization(strFN2);
				return true;
			}
			w8.Finalization(strFN2);
			return false;
		}
		w8 = null;
		WebCheck9.ClassFiscal w9 = All.W9;
		if (Operators.CompareString(w9.StatusFN(), "", false) == 0 && w9.Initialization(strFN2))
		{
			string text17 = "";
			if (num == 0)
			{
				text17 = w9.CheckLine.CheckXML(parametrToString.ReturnStr);
			}
			string text18 = "";
			if (num > 0)
			{
				string strFN11 = "<InputParameters><Parameters TaxNum='" + parametrToString.ReturnStr + "' Type='" + num + "' FN='" + typErrStr.FN + "'/></InputParameters>";
				if (w9.GetCheckByFiscalNumber(strFN11))
				{
					text18 = w9.StatusBarXML();
					text17 = "";
				}
			}
			if ((Operators.CompareString(text18.Trim(), "", false) == 0) & (Operators.CompareString(text17.Trim(), "", false) == 0))
			{
				All.Log.SaveTextToLog(typErrStr.FN, "GetCheck", strFN, parametrToString.ReturnStr + " - чек не найден");
				All.ReplyRemember(typErrStr.FN, All.XMLRiplyErr(typErrStr.FN, parametrToString.ReturnStr + " - чек на найден"));
				w9.Finalization(strFN2);
				return false;
			}
			if (All.ReplyRemember(typErrStr.FN, text18, text17))
			{
				w9.Finalization(strFN2);
				return true;
			}
			w9.Finalization(strFN2);
			return false;
		}
		w9 = null;
		WebCheck10.ClassFiscal w10 = All.W10;
		if (Operators.CompareString(w10.StatusFN(), "", false) == 0 && w10.Initialization(strFN2))
		{
			string text19 = "";
			if (num == 0)
			{
				text19 = w10.CheckLine.CheckXML(parametrToString.ReturnStr);
			}
			string text20 = "";
			if (num > 0)
			{
				string strFN12 = "<InputParameters><Parameters TaxNum='" + parametrToString.ReturnStr + "' Type='" + num + "' FN='" + typErrStr.FN + "'/></InputParameters>";
				if (w10.GetCheckByFiscalNumber(strFN12))
				{
					text20 = w10.StatusBarXML();
					text19 = "";
				}
			}
			if ((Operators.CompareString(text20.Trim(), "", false) == 0) & (Operators.CompareString(text19.Trim(), "", false) == 0))
			{
				All.Log.SaveTextToLog(typErrStr.FN, "GetCheck", strFN, parametrToString.ReturnStr + " - чек не найден");
				All.ReplyRemember(typErrStr.FN, All.XMLRiplyErr(typErrStr.FN, parametrToString.ReturnStr + " - чек на найден"));
				w10.Finalization(strFN2);
				return false;
			}
			if (All.ReplyRemember(typErrStr.FN, text20, text19))
			{
				w10.Finalization(strFN2);
				return true;
			}
			w10.Finalization(strFN2);
			return false;
		}
		w10 = null;
		WebCheck11.ClassFiscal w11 = All.W11;
		if (Operators.CompareString(w11.StatusFN(), "", false) == 0 && w11.Initialization(strFN2))
		{
			string text21 = "";
			if (num == 0)
			{
				text21 = w11.CheckLine.CheckXML(parametrToString.ReturnStr);
			}
			string text22 = "";
			if (num > 0)
			{
				string strFN13 = "<InputParameters><Parameters TaxNum='" + parametrToString.ReturnStr + "' Type='" + num + "' FN='" + typErrStr.FN + "'/></InputParameters>";
				if (w11.GetCheckByFiscalNumber(strFN13))
				{
					text22 = w11.StatusBarXML();
					text21 = "";
				}
			}
			if ((Operators.CompareString(text22.Trim(), "", false) == 0) & (Operators.CompareString(text21.Trim(), "", false) == 0))
			{
				All.Log.SaveTextToLog(typErrStr.FN, "GetCheck", strFN, parametrToString.ReturnStr + " - чек не найден");
				All.ReplyRemember(typErrStr.FN, All.XMLRiplyErr(typErrStr.FN, parametrToString.ReturnStr + " - чек на найден"));
				w11.Finalization(strFN2);
				return false;
			}
			if (All.ReplyRemember(typErrStr.FN, text22, text21))
			{
				w11.Finalization(strFN2);
				return true;
			}
			w11.Finalization(strFN2);
			return false;
		}
		w11 = null;
		WebCheck12.ClassFiscal w12 = All.W12;
		if (Operators.CompareString(w12.StatusFN(), "", false) == 0 && w12.Initialization(strFN2))
		{
			string text23 = "";
			if (num == 0)
			{
				text23 = w12.CheckLine.CheckXML(parametrToString.ReturnStr);
			}
			string text24 = "";
			if (num > 0)
			{
				string strFN14 = "<InputParameters><Parameters TaxNum='" + parametrToString.ReturnStr + "' Type='" + num + "' FN='" + typErrStr.FN + "'/></InputParameters>";
				if (w12.GetCheckByFiscalNumber(strFN14))
				{
					text24 = w12.StatusBarXML();
					text23 = "";
				}
			}
			if ((Operators.CompareString(text24.Trim(), "", false) == 0) & (Operators.CompareString(text23.Trim(), "", false) == 0))
			{
				All.Log.SaveTextToLog(typErrStr.FN, "GetCheck", strFN, parametrToString.ReturnStr + " - чек не найден");
				All.ReplyRemember(typErrStr.FN, All.XMLRiplyErr(typErrStr.FN, parametrToString.ReturnStr + " - чек на найден"));
				w12.Finalization(strFN2);
				return false;
			}
			if (All.ReplyRemember(typErrStr.FN, text24, text23))
			{
				w12.Finalization(strFN2);
				return true;
			}
			w12.Finalization(strFN2);
			return false;
		}
		w12 = null;
		WebCheck13.ClassFiscal w13 = All.W13;
		if (Operators.CompareString(w13.StatusFN(), "", false) == 0 && w13.Initialization(strFN2))
		{
			string text25 = "";
			if (num == 0)
			{
				text25 = w13.CheckLine.CheckXML(parametrToString.ReturnStr);
			}
			string text26 = "";
			if (num > 0)
			{
				string strFN15 = "<InputParameters><Parameters TaxNum='" + parametrToString.ReturnStr + "' Type='" + num + "' FN='" + typErrStr.FN + "'/></InputParameters>";
				if (w13.GetCheckByFiscalNumber(strFN15))
				{
					text26 = w13.StatusBarXML();
					text25 = "";
				}
			}
			if ((Operators.CompareString(text26.Trim(), "", false) == 0) & (Operators.CompareString(text25.Trim(), "", false) == 0))
			{
				All.Log.SaveTextToLog(typErrStr.FN, "GetCheck", strFN, parametrToString.ReturnStr + " - чек не найден");
				All.ReplyRemember(typErrStr.FN, All.XMLRiplyErr(typErrStr.FN, parametrToString.ReturnStr + " - чек на найден"));
				w13.Finalization(strFN2);
				return false;
			}
			if (All.ReplyRemember(typErrStr.FN, text26, text25))
			{
				w13.Finalization(strFN2);
				return true;
			}
			w13.Finalization(strFN2);
			return false;
		}
		w13 = null;
		WebCheck14.ClassFiscal w14 = All.W14;
		if (Operators.CompareString(w14.StatusFN(), "", false) == 0 && w14.Initialization(strFN2))
		{
			string text27 = "";
			if (num == 0)
			{
				text27 = w14.CheckLine.CheckXML(parametrToString.ReturnStr);
			}
			string text28 = "";
			if (num > 0)
			{
				string strFN16 = "<InputParameters><Parameters TaxNum='" + parametrToString.ReturnStr + "' Type='" + num + "' FN='" + typErrStr.FN + "'/></InputParameters>";
				if (w14.GetCheckByFiscalNumber(strFN16))
				{
					text28 = w14.StatusBarXML();
					text27 = "";
				}
			}
			if ((Operators.CompareString(text28.Trim(), "", false) == 0) & (Operators.CompareString(text27.Trim(), "", false) == 0))
			{
				All.Log.SaveTextToLog(typErrStr.FN, "GetCheck", strFN, parametrToString.ReturnStr + " - чек не найден");
				All.ReplyRemember(typErrStr.FN, All.XMLRiplyErr(typErrStr.FN, parametrToString.ReturnStr + " - чек на найден"));
				w14.Finalization(strFN2);
				return false;
			}
			if (All.ReplyRemember(typErrStr.FN, text28, text27))
			{
				w14.Finalization(strFN2);
				return true;
			}
			w14.Finalization(strFN2);
			return false;
		}
		w14 = null;
		WebCheck15.ClassFiscal w15 = All.W15;
		if (Operators.CompareString(w15.StatusFN(), "", false) == 0 && w15.Initialization(strFN2))
		{
			string text29 = "";
			if (num == 0)
			{
				text29 = w15.CheckLine.CheckXML(parametrToString.ReturnStr);
			}
			string text30 = "";
			if (num > 0)
			{
				string strFN17 = "<InputParameters><Parameters TaxNum='" + parametrToString.ReturnStr + "' Type='" + num + "' FN='" + typErrStr.FN + "'/></InputParameters>";
				if (w15.GetCheckByFiscalNumber(strFN17))
				{
					text30 = w15.StatusBarXML();
					text29 = "";
				}
			}
			if ((Operators.CompareString(text30.Trim(), "", false) == 0) & (Operators.CompareString(text29.Trim(), "", false) == 0))
			{
				All.Log.SaveTextToLog(typErrStr.FN, "GetCheck", strFN, parametrToString.ReturnStr + " - чек не найден");
				All.ReplyRemember(typErrStr.FN, All.XMLRiplyErr(typErrStr.FN, parametrToString.ReturnStr + " - чек на найден"));
				w15.Finalization(strFN2);
				return false;
			}
			if (All.ReplyRemember(typErrStr.FN, text30, text29))
			{
				w15.Finalization(strFN2);
				return true;
			}
			w15.Finalization(strFN2);
			return false;
		}
		w15 = null;
		WebCheck16.ClassFiscal w16 = All.W16;
		if (Operators.CompareString(w16.StatusFN(), "", false) == 0 && w16.Initialization(strFN2))
		{
			string text31 = "";
			if (num == 0)
			{
				text31 = w16.CheckLine.CheckXML(parametrToString.ReturnStr);
			}
			string text32 = "";
			if (num > 0)
			{
				string strFN18 = "<InputParameters><Parameters TaxNum='" + parametrToString.ReturnStr + "' Type='" + num + "' FN='" + typErrStr.FN + "'/></InputParameters>";
				if (w16.GetCheckByFiscalNumber(strFN18))
				{
					text32 = w16.StatusBarXML();
					text31 = "";
				}
			}
			if ((Operators.CompareString(text32.Trim(), "", false) == 0) & (Operators.CompareString(text31.Trim(), "", false) == 0))
			{
				All.Log.SaveTextToLog(typErrStr.FN, "GetCheck", strFN, parametrToString.ReturnStr + " - чек не найден");
				All.ReplyRemember(typErrStr.FN, All.XMLRiplyErr(typErrStr.FN, parametrToString.ReturnStr + " - чек на найден"));
				w16.Finalization(strFN2);
				return false;
			}
			if (All.ReplyRemember(typErrStr.FN, text32, text31))
			{
				w16.Finalization(strFN2);
				return true;
			}
			w16.Finalization(strFN2);
			return false;
		}
		w16 = null;
		WebCheck17.ClassFiscal w17 = All.W17;
		if (Operators.CompareString(w17.StatusFN(), "", false) == 0 && w17.Initialization(strFN2))
		{
			string text33 = "";
			if (num == 0)
			{
				text33 = w17.CheckLine.CheckXML(parametrToString.ReturnStr);
			}
			string text34 = "";
			if (num > 0)
			{
				string strFN19 = "<InputParameters><Parameters TaxNum='" + parametrToString.ReturnStr + "' Type='" + num + "' FN='" + typErrStr.FN + "'/></InputParameters>";
				if (w17.GetCheckByFiscalNumber(strFN19))
				{
					text34 = w17.StatusBarXML();
					text33 = "";
				}
			}
			if ((Operators.CompareString(text34.Trim(), "", false) == 0) & (Operators.CompareString(text33.Trim(), "", false) == 0))
			{
				All.Log.SaveTextToLog(typErrStr.FN, "GetCheck", strFN, parametrToString.ReturnStr + " - чек не найден");
				All.ReplyRemember(typErrStr.FN, All.XMLRiplyErr(typErrStr.FN, parametrToString.ReturnStr + " - чек на найден"));
				w17.Finalization(strFN2);
				return false;
			}
			if (All.ReplyRemember(typErrStr.FN, text34, text33))
			{
				w17.Finalization(strFN2);
				return true;
			}
			w17.Finalization(strFN2);
			return false;
		}
		w17 = null;
		WebCheck18.ClassFiscal w18 = All.W18;
		if (Operators.CompareString(w18.StatusFN(), "", false) == 0 && w18.Initialization(strFN2))
		{
			string text35 = "";
			if (num == 0)
			{
				text35 = w18.CheckLine.CheckXML(parametrToString.ReturnStr);
			}
			string text36 = "";
			if (num > 0)
			{
				string strFN20 = "<InputParameters><Parameters TaxNum='" + parametrToString.ReturnStr + "' Type='" + num + "' FN='" + typErrStr.FN + "'/></InputParameters>";
				if (w18.GetCheckByFiscalNumber(strFN20))
				{
					text36 = w18.StatusBarXML();
					text35 = "";
				}
			}
			if ((Operators.CompareString(text36.Trim(), "", false) == 0) & (Operators.CompareString(text35.Trim(), "", false) == 0))
			{
				All.Log.SaveTextToLog(typErrStr.FN, "GetCheck", strFN, parametrToString.ReturnStr + " - чек не найден");
				All.ReplyRemember(typErrStr.FN, All.XMLRiplyErr(typErrStr.FN, parametrToString.ReturnStr + " - чек на найден"));
				w18.Finalization(strFN2);
				return false;
			}
			if (All.ReplyRemember(typErrStr.FN, text36, text35))
			{
				w18.Finalization(strFN2);
				return true;
			}
			w18.Finalization(strFN2);
			return false;
		}
		w18 = null;
		All.Log.SaveTextToLog(typErrStr.FN, "GetCheck", strFN, "Все слоты заняты транзакциями с сервером налоговой");
		All.ReplyRemember(typErrStr.FN, All.XMLRiplyErr(typErrStr.FN, "Все слоты заняты транзакциями с сервером налоговой"));
		return false;
	}

	public bool GetCheckEX(string strFN, ref string xmlS)
	{
		TypErrStr typErrStr = All.TestFN(strFN, "", TransactionControl: false);
		if (typErrStr.errCode > 0)
		{
			All.ReplyRemember(typErrStr.FN, All.XMLRiplyErr(typErrStr.FN, typErrStr.errStr));
			All.Log.SaveTextToLog(typErrStr.FN, "GetCheckEX", strFN, typErrStr.errStr);
			return false;
		}
		TypErrStr parametrToString = All.GetParametrToString(strFN, "TaxNum", "InputParameters/Parameters", RegUpLow: true);
		if (parametrToString.errCode > 0)
		{
			parametrToString = All.GetParametrToString(strFN, "taxnum", "InputParameters/Parameters", RegUpLow: true);
		}
		if (parametrToString.errCode > 0)
		{
			parametrToString = All.GetParametrToString(strFN, "Taxnum", "InputParameters/Parameters", RegUpLow: true);
		}
		if (parametrToString.errCode > 0)
		{
			parametrToString = All.GetParametrToString(strFN, "TAXNUM", "InputParameters/Parameters", RegUpLow: true);
		}
		if (parametrToString.errCode > 0)
		{
			parametrToString.ReturnStr = "";
		}
		WebCheck.ClassFiscal classFiscal = new WebCheck.ClassFiscal();
		xmlS = classFiscal.ServerSetGet.GetCheckByFiscalNumberEX(parametrToString.ReturnStr, typErrStr.FN);
		if (Operators.CompareString(xmlS.Trim(), "", false) == 0)
		{
			return false;
		}
		return true;
	}

	public bool GetShiftStatusEX(string strFN, ref string xmlS)
	{
		TypErrStr typErrStr = All.TestFN(strFN, "", TransactionControl: false);
		if (typErrStr.errCode > 0)
		{
			All.ReplyRemember(typErrStr.FN, All.XMLRiplyErr(typErrStr.FN, typErrStr.errStr));
			All.Log.SaveTextToLog(typErrStr.FN, "GetShiftStatusEX", strFN, typErrStr.errStr);
			return false;
		}
		WebCheck.ClassFiscal classFiscal = new WebCheck.ClassFiscal();
		xmlS = classFiscal.ServerSetGet.GetShiftStatusEX(typErrStr.FN);
		if (Operators.CompareString(xmlS.Trim(), "", false) == 0)
		{
			return false;
		}
		return true;
	}

	public bool GetDocumentsByShift(string strFN)
	{
		TypErrStr typErrStr = All.TestFN(strFN);
		if (typErrStr.errCode > 0)
		{
			All.ReplyRemember(typErrStr.FN, All.XMLRiplyErr(typErrStr.FN, typErrStr.errStr));
			All.Log.SaveTextToLog(typErrStr.FN, "GetDocumentsByShift", strFN, typErrStr.errStr);
			return false;
		}
		TypErrStr parametrToString = All.GetParametrToString(strFN, "shiftId");
		if (parametrToString.errCode > 0)
		{
			All.ReplyRemember(typErrStr.FN, All.XMLRiplyErr(typErrStr.FN, parametrToString.errStr));
			All.Log.SaveTextToLog(typErrStr.FN, "GetDocumentsByShift", strFN, parametrToString.errStr);
			return false;
		}
		if (Versioned.IsNumeric((object)parametrToString.ReturnStr))
		{
			int shiftID = Conversions.ToInteger(parametrToString.ReturnStr);
			string strFN2 = "<InputParameters><Parameters FN='" + typErrStr.FN + "'/></InputParameters>";
			WebCheck1.ClassFiscal w = All.W1;
			if (Operators.CompareString(w.StatusFN(), "", false) == 0 && w.Initialization(strFN2))
			{
				string documentsByShifts = w.ServerSetGet.GetDocumentsByShifts(shiftID);
				if (Operators.CompareString(documentsByShifts, "", false) == 0)
				{
					All.Log.SaveTextToLog(typErrStr.FN, "GetDocumentsByShift", strFN, "Чеки не найдены");
					w.Finalization(strFN2);
					return false;
				}
				if (All.ReplyRemember(typErrStr.FN, documentsByShifts))
				{
					w.Finalization(strFN2);
					return true;
				}
				w.Finalization(strFN2);
				return false;
			}
			w = null;
			WebCheck2.ClassFiscal w2 = All.W2;
			if (Operators.CompareString(w2.StatusFN(), "", false) == 0 && w2.Initialization(strFN2))
			{
				string documentsByShifts2 = w2.ServerSetGet.GetDocumentsByShifts(shiftID);
				if (Operators.CompareString(documentsByShifts2, "", false) == 0)
				{
					All.Log.SaveTextToLog(typErrStr.FN, "GetDocumentsByShift", strFN, "Чеки не найдены");
					w2.Finalization(strFN2);
					return false;
				}
				if (All.ReplyRemember(typErrStr.FN, documentsByShifts2))
				{
					w2.Finalization(strFN2);
					return true;
				}
				w2.Finalization(strFN2);
				return false;
			}
			w2 = null;
			WebCheck3.ClassFiscal w3 = All.W3;
			if (Operators.CompareString(w3.StatusFN(), "", false) == 0 && w3.Initialization(strFN2))
			{
				string documentsByShifts3 = w3.ServerSetGet.GetDocumentsByShifts(shiftID);
				if (Operators.CompareString(documentsByShifts3, "", false) == 0)
				{
					All.Log.SaveTextToLog(typErrStr.FN, "GetDocumentsByShift", strFN, "Чеки не найдены");
					w3.Finalization(strFN2);
					return false;
				}
				if (All.ReplyRemember(typErrStr.FN, documentsByShifts3))
				{
					w3.Finalization(strFN2);
					return true;
				}
				w3.Finalization(strFN2);
				return false;
			}
			w3 = null;
			WebCheck4.ClassFiscal w4 = All.W4;
			if (Operators.CompareString(w4.StatusFN(), "", false) == 0 && w4.Initialization(strFN2))
			{
				string documentsByShifts4 = w4.ServerSetGet.GetDocumentsByShifts(shiftID);
				if (Operators.CompareString(documentsByShifts4, "", false) == 0)
				{
					All.Log.SaveTextToLog(typErrStr.FN, "GetDocumentsByShift", strFN, "Чеки не найдены");
					w4.Finalization(strFN2);
					return false;
				}
				if (All.ReplyRemember(typErrStr.FN, documentsByShifts4))
				{
					w4.Finalization(strFN2);
					return true;
				}
				w4.Finalization(strFN2);
				return false;
			}
			w4 = null;
			WebCheck5.ClassFiscal w5 = All.W5;
			if (Operators.CompareString(w5.StatusFN(), "", false) == 0 && w5.Initialization(strFN2))
			{
				string documentsByShifts5 = w5.ServerSetGet.GetDocumentsByShifts(shiftID);
				if (Operators.CompareString(documentsByShifts5, "", false) == 0)
				{
					All.Log.SaveTextToLog(typErrStr.FN, "GetDocumentsByShift", strFN, "Чеки не найдены");
					w5.Finalization(strFN2);
					return false;
				}
				if (All.ReplyRemember(typErrStr.FN, documentsByShifts5))
				{
					w5.Finalization(strFN2);
					return true;
				}
				w5.Finalization(strFN2);
				return false;
			}
			w5 = null;
			WebCheck6.ClassFiscal w6 = All.W6;
			if (Operators.CompareString(w6.StatusFN(), "", false) == 0 && w6.Initialization(strFN2))
			{
				string documentsByShifts6 = w6.ServerSetGet.GetDocumentsByShifts(shiftID);
				if (Operators.CompareString(documentsByShifts6, "", false) == 0)
				{
					All.Log.SaveTextToLog(typErrStr.FN, "GetDocumentsByShift", strFN, "Чеки не найдены");
					w6.Finalization(strFN2);
					return false;
				}
				if (All.ReplyRemember(typErrStr.FN, documentsByShifts6))
				{
					w6.Finalization(strFN2);
					return true;
				}
				w6.Finalization(strFN2);
				return false;
			}
			w6 = null;
			WebCheck7.ClassFiscal w7 = All.W7;
			if (Operators.CompareString(w7.StatusFN(), "", false) == 0 && w7.Initialization(strFN2))
			{
				string documentsByShifts7 = w7.ServerSetGet.GetDocumentsByShifts(shiftID);
				if (Operators.CompareString(documentsByShifts7, "", false) == 0)
				{
					All.Log.SaveTextToLog(typErrStr.FN, "GetDocumentsByShift", strFN, "Чеки не найдены");
					w7.Finalization(strFN2);
					return false;
				}
				if (All.ReplyRemember(typErrStr.FN, documentsByShifts7))
				{
					w7.Finalization(strFN2);
					return true;
				}
				w7.Finalization(strFN2);
				return false;
			}
			w7 = null;
			WebCheck8.ClassFiscal w8 = All.W8;
			if (Operators.CompareString(w8.StatusFN(), "", false) == 0 && w8.Initialization(strFN2))
			{
				string documentsByShifts8 = w8.ServerSetGet.GetDocumentsByShifts(shiftID);
				if (Operators.CompareString(documentsByShifts8, "", false) == 0)
				{
					All.Log.SaveTextToLog(typErrStr.FN, "GetDocumentsByShift", strFN, "Чеки не найдены");
					w8.Finalization(strFN2);
					return false;
				}
				if (All.ReplyRemember(typErrStr.FN, documentsByShifts8))
				{
					w8.Finalization(strFN2);
					return true;
				}
				w8.Finalization(strFN2);
				return false;
			}
			w8 = null;
			WebCheck9.ClassFiscal w9 = All.W9;
			if (Operators.CompareString(w9.StatusFN(), "", false) == 0 && w9.Initialization(strFN2))
			{
				string documentsByShifts9 = w9.ServerSetGet.GetDocumentsByShifts(shiftID);
				if (Operators.CompareString(documentsByShifts9, "", false) == 0)
				{
					All.Log.SaveTextToLog(typErrStr.FN, "GetDocumentsByShift", strFN, "Чеки не найдены");
					w9.Finalization(strFN2);
					return false;
				}
				if (All.ReplyRemember(typErrStr.FN, documentsByShifts9))
				{
					w9.Finalization(strFN2);
					return true;
				}
				w9.Finalization(strFN2);
				return false;
			}
			w9 = null;
			WebCheck10.ClassFiscal w10 = All.W10;
			if (Operators.CompareString(w10.StatusFN(), "", false) == 0 && w10.Initialization(strFN2))
			{
				string documentsByShifts10 = w10.ServerSetGet.GetDocumentsByShifts(shiftID);
				if (Operators.CompareString(documentsByShifts10, "", false) == 0)
				{
					All.Log.SaveTextToLog(typErrStr.FN, "GetDocumentsByShift", strFN, "Чеки не найдены");
					w10.Finalization(strFN2);
					return false;
				}
				if (All.ReplyRemember(typErrStr.FN, documentsByShifts10))
				{
					w10.Finalization(strFN2);
					return true;
				}
				w10.Finalization(strFN2);
				return false;
			}
			w10 = null;
			WebCheck11.ClassFiscal w11 = All.W11;
			if (Operators.CompareString(w11.StatusFN(), "", false) == 0 && w11.Initialization(strFN2))
			{
				string documentsByShifts11 = w11.ServerSetGet.GetDocumentsByShifts(shiftID);
				if (Operators.CompareString(documentsByShifts11, "", false) == 0)
				{
					All.Log.SaveTextToLog(typErrStr.FN, "GetDocumentsByShift", strFN, "Чеки не найдены");
					w11.Finalization(strFN2);
					return false;
				}
				if (All.ReplyRemember(typErrStr.FN, documentsByShifts11))
				{
					w11.Finalization(strFN2);
					return true;
				}
				w11.Finalization(strFN2);
				return false;
			}
			w11 = null;
			WebCheck12.ClassFiscal w12 = All.W12;
			if (Operators.CompareString(w12.StatusFN(), "", false) == 0 && w12.Initialization(strFN2))
			{
				string documentsByShifts12 = w12.ServerSetGet.GetDocumentsByShifts(shiftID);
				if (Operators.CompareString(documentsByShifts12, "", false) == 0)
				{
					All.Log.SaveTextToLog(typErrStr.FN, "GetDocumentsByShift", strFN, "Чеки не найдены");
					w12.Finalization(strFN2);
					return false;
				}
				if (All.ReplyRemember(typErrStr.FN, documentsByShifts12))
				{
					w12.Finalization(strFN2);
					return true;
				}
				w12.Finalization(strFN2);
				return false;
			}
			w12 = null;
			WebCheck13.ClassFiscal w13 = All.W13;
			if (Operators.CompareString(w13.StatusFN(), "", false) == 0 && w13.Initialization(strFN2))
			{
				string documentsByShifts13 = w13.ServerSetGet.GetDocumentsByShifts(shiftID);
				if (Operators.CompareString(documentsByShifts13, "", false) == 0)
				{
					All.Log.SaveTextToLog(typErrStr.FN, "GetDocumentsByShift", strFN, "Чеки не найдены");
					w13.Finalization(strFN2);
					return false;
				}
				if (All.ReplyRemember(typErrStr.FN, documentsByShifts13))
				{
					w13.Finalization(strFN2);
					return true;
				}
				w13.Finalization(strFN2);
				return false;
			}
			w13 = null;
			WebCheck14.ClassFiscal w14 = All.W14;
			if (Operators.CompareString(w14.StatusFN(), "", false) == 0 && w14.Initialization(strFN2))
			{
				string documentsByShifts14 = w14.ServerSetGet.GetDocumentsByShifts(shiftID);
				if (Operators.CompareString(documentsByShifts14, "", false) == 0)
				{
					All.Log.SaveTextToLog(typErrStr.FN, "GetDocumentsByShift", strFN, "Чеки не найдены");
					w14.Finalization(strFN2);
					return false;
				}
				if (All.ReplyRemember(typErrStr.FN, documentsByShifts14))
				{
					w14.Finalization(strFN2);
					return true;
				}
				w14.Finalization(strFN2);
				return false;
			}
			w14 = null;
			WebCheck15.ClassFiscal w15 = All.W15;
			if (Operators.CompareString(w15.StatusFN(), "", false) == 0 && w15.Initialization(strFN2))
			{
				string documentsByShifts15 = w15.ServerSetGet.GetDocumentsByShifts(shiftID);
				if (Operators.CompareString(documentsByShifts15, "", false) == 0)
				{
					All.Log.SaveTextToLog(typErrStr.FN, "GetDocumentsByShift", strFN, "Чеки не найдены");
					w15.Finalization(strFN2);
					return false;
				}
				if (All.ReplyRemember(typErrStr.FN, documentsByShifts15))
				{
					w15.Finalization(strFN2);
					return true;
				}
				w15.Finalization(strFN2);
				return false;
			}
			w15 = null;
			WebCheck16.ClassFiscal w16 = All.W16;
			if (Operators.CompareString(w16.StatusFN(), "", false) == 0 && w16.Initialization(strFN2))
			{
				string documentsByShifts16 = w16.ServerSetGet.GetDocumentsByShifts(shiftID);
				if (Operators.CompareString(documentsByShifts16, "", false) == 0)
				{
					All.Log.SaveTextToLog(typErrStr.FN, "GetDocumentsByShift", strFN, "Чеки не найдены");
					w16.Finalization(strFN2);
					return false;
				}
				if (All.ReplyRemember(typErrStr.FN, documentsByShifts16))
				{
					w16.Finalization(strFN2);
					return true;
				}
				w16.Finalization(strFN2);
				return false;
			}
			w16 = null;
			WebCheck17.ClassFiscal w17 = All.W17;
			if (Operators.CompareString(w17.StatusFN(), "", false) == 0 && w17.Initialization(strFN2))
			{
				string documentsByShifts17 = w17.ServerSetGet.GetDocumentsByShifts(shiftID);
				if (Operators.CompareString(documentsByShifts17, "", false) == 0)
				{
					All.Log.SaveTextToLog(typErrStr.FN, "GetDocumentsByShift", strFN, "Чеки не найдены");
					w17.Finalization(strFN2);
					return false;
				}
				if (All.ReplyRemember(typErrStr.FN, documentsByShifts17))
				{
					w17.Finalization(strFN2);
					return true;
				}
				w17.Finalization(strFN2);
				return false;
			}
			w17 = null;
			WebCheck18.ClassFiscal w18 = All.W18;
			if (Operators.CompareString(w18.StatusFN(), "", false) == 0 && w18.Initialization(strFN2))
			{
				string documentsByShifts18 = w18.ServerSetGet.GetDocumentsByShifts(shiftID);
				if (Operators.CompareString(documentsByShifts18, "", false) == 0)
				{
					All.Log.SaveTextToLog(typErrStr.FN, "GetDocumentsByShift", strFN, "Чеки не найдены");
					w18.Finalization(strFN2);
					return false;
				}
				if (All.ReplyRemember(typErrStr.FN, documentsByShifts18))
				{
					w18.Finalization(strFN2);
					return true;
				}
				w18.Finalization(strFN2);
				return false;
			}
			w18 = null;
			All.Log.SaveTextToLog(typErrStr.FN, "GetDocumentsByShift", strFN, "Все слоты заняты");
			All.ReplyRemember(typErrStr.FN, All.XMLRiplyErr(typErrStr.FN, "Все слоты заняты"));
			return false;
		}
		All.ReplyRemember(typErrStr.FN, All.XMLRiplyErr(typErrStr.FN, "Не указан номер смены"));
		All.Log.SaveTextToLog(typErrStr.FN, "GetDocumentsByShift", strFN, "Не правильно указан номер смены");
		return false;
	}

	public bool GetDocumentsByShiftEX(string strFN, ref string xmlS)
	{
		TypErrStr typErrStr = All.TestFN(strFN, "", TransactionControl: false);
		if (typErrStr.errCode > 0)
		{
			All.ReplyRemember(typErrStr.FN, All.XMLRiplyErr(typErrStr.FN, typErrStr.errStr));
			All.Log.SaveTextToLog(typErrStr.FN, "GetDocumentsByShiftEX", strFN, typErrStr.errStr);
			return false;
		}
		TypErrStr parametrToString = All.GetParametrToString(strFN, "shiftId");
		if (parametrToString.errCode > 0)
		{
			All.ReplyRemember(typErrStr.FN, All.XMLRiplyErr(typErrStr.FN, parametrToString.errStr));
			All.Log.SaveTextToLog(typErrStr.FN, "GetDocumentsByShiftEX", strFN, parametrToString.errStr);
			return false;
		}
		if (Versioned.IsNumeric((object)parametrToString.ReturnStr))
		{
			int shiftID = Conversions.ToInteger(parametrToString.ReturnStr);
			WebCheck.ClassFiscal classFiscal = new WebCheck.ClassFiscal();
			xmlS = classFiscal.ServerSetGet.GetDocumentsByShiftsEX(shiftID, typErrStr.FN);
			if (Operators.CompareString(xmlS.Trim(), "", false) == 0)
			{
				return false;
			}
			return true;
		}
		All.ReplyRemember(typErrStr.FN, All.XMLRiplyErr(typErrStr.FN, "Не указан номер смены"));
		All.Log.SaveTextToLog(typErrStr.FN, "GetDocumentsByShiftEX", strFN, "Не правильно указан номер смены");
		return false;
	}

	public bool GetShifts(string strFN)
	{
		TypErrStr typErrStr = All.TestFN(strFN);
		if (typErrStr.errCode > 0)
		{
			All.ReplyRemember(typErrStr.FN, All.XMLRiplyErr(typErrStr.FN, typErrStr.errStr));
			All.Log.SaveTextToLog(typErrStr.FN, "GetShifts", strFN, typErrStr.errStr);
			return false;
		}
		TypErrStr parametrToString = All.GetParametrToString(strFN, "shiftmonth");
		if (parametrToString.errCode > 0)
		{
			All.ReplyRemember(typErrStr.FN, All.XMLRiplyErr(typErrStr.FN, parametrToString.errStr));
			All.Log.SaveTextToLog(typErrStr.FN, "GetShifts", strFN, parametrToString.errStr);
			return false;
		}
		string returnStr = parametrToString.ReturnStr;
		string strFN2 = "<InputParameters><Parameters FN='" + typErrStr.FN + "'/></InputParameters>";
		WebCheck1.ClassFiscal w = All.W1;
		if (Operators.CompareString(w.StatusFN(), "", false) == 0 && w.Initialization(strFN2))
		{
			string shiftsDate = w.ServerSetGet.GetShiftsDate(returnStr);
			if (Operators.CompareString(shiftsDate, "", false) == 0)
			{
				All.Log.SaveTextToLog(typErrStr.FN, "GetShifts", strFN, "Смены не найдены");
				w.Finalization(strFN2);
				return false;
			}
			if (All.ReplyRemember(typErrStr.FN, shiftsDate))
			{
				w.Finalization(strFN2);
				return true;
			}
			w.Finalization(strFN2);
			return false;
		}
		w = null;
		WebCheck2.ClassFiscal w2 = All.W2;
		if (Operators.CompareString(w2.StatusFN(), "", false) == 0 && w2.Initialization(strFN2))
		{
			string shiftsDate2 = w2.ServerSetGet.GetShiftsDate(returnStr);
			if (Operators.CompareString(shiftsDate2, "", false) == 0)
			{
				All.Log.SaveTextToLog(typErrStr.FN, "GetShifts", strFN, "Смены не найдены");
				w2.Finalization(strFN2);
				return false;
			}
			if (All.ReplyRemember(typErrStr.FN, shiftsDate2))
			{
				w2.Finalization(strFN2);
				return true;
			}
			w2.Finalization(strFN2);
			return false;
		}
		w2 = null;
		WebCheck3.ClassFiscal w3 = All.W3;
		if (Operators.CompareString(w3.StatusFN(), "", false) == 0 && w3.Initialization(strFN2))
		{
			string shiftsDate3 = w3.ServerSetGet.GetShiftsDate(returnStr);
			if (Operators.CompareString(shiftsDate3, "", false) == 0)
			{
				All.Log.SaveTextToLog(typErrStr.FN, "GetShifts", strFN, "Смены не найдены");
				w3.Finalization(strFN2);
				return false;
			}
			if (All.ReplyRemember(typErrStr.FN, shiftsDate3))
			{
				w3.Finalization(strFN2);
				return true;
			}
			w3.Finalization(strFN2);
			return false;
		}
		w3 = null;
		WebCheck4.ClassFiscal w4 = All.W4;
		if (Operators.CompareString(w4.StatusFN(), "", false) == 0 && w4.Initialization(strFN2))
		{
			string shiftsDate4 = w4.ServerSetGet.GetShiftsDate(returnStr);
			if (Operators.CompareString(shiftsDate4, "", false) == 0)
			{
				All.Log.SaveTextToLog(typErrStr.FN, "GetShifts", strFN, "Смены не найдены");
				w4.Finalization(strFN2);
				return false;
			}
			if (All.ReplyRemember(typErrStr.FN, shiftsDate4))
			{
				w4.Finalization(strFN2);
				return true;
			}
			w4.Finalization(strFN2);
			return false;
		}
		w4 = null;
		WebCheck5.ClassFiscal w5 = All.W5;
		if (Operators.CompareString(w5.StatusFN(), "", false) == 0 && w5.Initialization(strFN2))
		{
			string shiftsDate5 = w5.ServerSetGet.GetShiftsDate(returnStr);
			if (Operators.CompareString(shiftsDate5, "", false) == 0)
			{
				All.Log.SaveTextToLog(typErrStr.FN, "GetShifts", strFN, "Смены не найдены");
				w5.Finalization(strFN2);
				return false;
			}
			if (All.ReplyRemember(typErrStr.FN, shiftsDate5))
			{
				w5.Finalization(strFN2);
				return true;
			}
			w5.Finalization(strFN2);
			return false;
		}
		w5 = null;
		WebCheck6.ClassFiscal w6 = All.W6;
		if (Operators.CompareString(w6.StatusFN(), "", false) == 0 && w6.Initialization(strFN2))
		{
			string shiftsDate6 = w6.ServerSetGet.GetShiftsDate(returnStr);
			if (Operators.CompareString(shiftsDate6, "", false) == 0)
			{
				All.Log.SaveTextToLog(typErrStr.FN, "GetShifts", strFN, "Смены не найдены");
				w6.Finalization(strFN2);
				return false;
			}
			if (All.ReplyRemember(typErrStr.FN, shiftsDate6))
			{
				w6.Finalization(strFN2);
				return true;
			}
			w6.Finalization(strFN2);
			return false;
		}
		w6 = null;
		WebCheck7.ClassFiscal w7 = All.W7;
		if (Operators.CompareString(w7.StatusFN(), "", false) == 0 && w7.Initialization(strFN2))
		{
			string shiftsDate7 = w7.ServerSetGet.GetShiftsDate(returnStr);
			if (Operators.CompareString(shiftsDate7, "", false) == 0)
			{
				All.Log.SaveTextToLog(typErrStr.FN, "GetShifts", strFN, "Смены не найдены");
				w7.Finalization(strFN2);
				return false;
			}
			if (All.ReplyRemember(typErrStr.FN, shiftsDate7))
			{
				w7.Finalization(strFN2);
				return true;
			}
			w7.Finalization(strFN2);
			return false;
		}
		w7 = null;
		WebCheck8.ClassFiscal w8 = All.W8;
		if (Operators.CompareString(w8.StatusFN(), "", false) == 0 && w8.Initialization(strFN2))
		{
			string shiftsDate8 = w8.ServerSetGet.GetShiftsDate(returnStr);
			if (Operators.CompareString(shiftsDate8, "", false) == 0)
			{
				All.Log.SaveTextToLog(typErrStr.FN, "GetShifts", strFN, "Смены не найдены");
				w8.Finalization(strFN2);
				return false;
			}
			if (All.ReplyRemember(typErrStr.FN, shiftsDate8))
			{
				w8.Finalization(strFN2);
				return true;
			}
			w8.Finalization(strFN2);
			return false;
		}
		w8 = null;
		WebCheck9.ClassFiscal w9 = All.W9;
		if (Operators.CompareString(w9.StatusFN(), "", false) == 0 && w9.Initialization(strFN2))
		{
			string shiftsDate9 = w9.ServerSetGet.GetShiftsDate(returnStr);
			if (Operators.CompareString(shiftsDate9, "", false) == 0)
			{
				All.Log.SaveTextToLog(typErrStr.FN, "GetShifts", strFN, "Смены не найдены");
				w9.Finalization(strFN2);
				return false;
			}
			if (All.ReplyRemember(typErrStr.FN, shiftsDate9))
			{
				w9.Finalization(strFN2);
				return true;
			}
			w9.Finalization(strFN2);
			return false;
		}
		w9 = null;
		WebCheck10.ClassFiscal w10 = All.W10;
		if (Operators.CompareString(w10.StatusFN(), "", false) == 0 && w10.Initialization(strFN2))
		{
			string shiftsDate10 = w10.ServerSetGet.GetShiftsDate(returnStr);
			if (Operators.CompareString(shiftsDate10, "", false) == 0)
			{
				All.Log.SaveTextToLog(typErrStr.FN, "GetShifts", strFN, "Смены не найдены");
				w10.Finalization(strFN2);
				return false;
			}
			if (All.ReplyRemember(typErrStr.FN, shiftsDate10))
			{
				w10.Finalization(strFN2);
				return true;
			}
			w10.Finalization(strFN2);
			return false;
		}
		w10 = null;
		WebCheck11.ClassFiscal w11 = All.W11;
		if (Operators.CompareString(w11.StatusFN(), "", false) == 0 && w11.Initialization(strFN2))
		{
			string shiftsDate11 = w11.ServerSetGet.GetShiftsDate(returnStr);
			if (Operators.CompareString(shiftsDate11, "", false) == 0)
			{
				All.Log.SaveTextToLog(typErrStr.FN, "GetShifts", strFN, "Смены не найдены");
				w11.Finalization(strFN2);
				return false;
			}
			if (All.ReplyRemember(typErrStr.FN, shiftsDate11))
			{
				w11.Finalization(strFN2);
				return true;
			}
			w11.Finalization(strFN2);
			return false;
		}
		w11 = null;
		WebCheck12.ClassFiscal w12 = All.W12;
		if (Operators.CompareString(w12.StatusFN(), "", false) == 0 && w12.Initialization(strFN2))
		{
			string shiftsDate12 = w12.ServerSetGet.GetShiftsDate(returnStr);
			if (Operators.CompareString(shiftsDate12, "", false) == 0)
			{
				All.Log.SaveTextToLog(typErrStr.FN, "GetShifts", strFN, "Смены не найдены");
				w12.Finalization(strFN2);
				return false;
			}
			if (All.ReplyRemember(typErrStr.FN, shiftsDate12))
			{
				w12.Finalization(strFN2);
				return true;
			}
			w12.Finalization(strFN2);
			return false;
		}
		w12 = null;
		All.Log.SaveTextToLog(typErrStr.FN, "GetShifts", strFN, "Все слоты заняты");
		All.ReplyRemember(typErrStr.FN, All.XMLRiplyErr(typErrStr.FN, "Все слоты заняты"));
		return false;
	}

	public bool GetShiftsEX(string strFN, ref string xmlS)
	{
		TypErrStr typErrStr = All.TestFN(strFN, "", TransactionControl: false);
		if (typErrStr.errCode > 0)
		{
			All.ReplyRemember(typErrStr.FN, All.XMLRiplyErr(typErrStr.FN, typErrStr.errStr));
			All.Log.SaveTextToLog(typErrStr.FN, "GetShiftsEX", strFN, typErrStr.errStr);
			return false;
		}
		TypErrStr parametrToString = All.GetParametrToString(strFN, "shiftmonth");
		if (parametrToString.errCode > 0)
		{
			All.ReplyRemember(typErrStr.FN, All.XMLRiplyErr(typErrStr.FN, parametrToString.errStr));
			All.Log.SaveTextToLog(typErrStr.FN, "GetShiftsEX", strFN, parametrToString.errStr);
			return false;
		}
		string returnStr = parametrToString.ReturnStr;
		WebCheck.ClassFiscal classFiscal = new WebCheck.ClassFiscal();
		xmlS = classFiscal.ServerSetGet.GetShiftsDateEX(returnStr, typErrStr.FN);
		if (Operators.CompareString(xmlS.Trim(), "", false) == 0)
		{
			return false;
		}
		return true;
	}

	public bool GetCashEX(string strFN, ref string xmlS)
	{
		TypErrStr typErrStr = All.TestFN(strFN, "", TransactionControl: false);
		if (typErrStr.errCode > 0)
		{
			All.ReplyRemember(typErrStr.FN, All.XMLRiplyErr(typErrStr.FN, typErrStr.errStr));
			All.Log.SaveTextToLog(typErrStr.FN, "GetShiftsEX", strFN, typErrStr.errStr);
			return false;
		}
		WebCheck.ClassFiscal classFiscal = new WebCheck.ClassFiscal();
		xmlS = classFiscal.ServerSetGet.GetCashEX(typErrStr.FN);
		if (Operators.CompareString(xmlS.Trim(), "", false) == 0)
		{
			return false;
		}
		return true;
	}

	public bool GetPeriodReport(string strFN)
	{
		TypErrStr typErrStr = All.TestFN(strFN);
		if (typErrStr.errCode > 0)
		{
			All.ReplyRemember(typErrStr.FN, All.XMLRiplyErr(typErrStr.FN, typErrStr.errStr));
			All.Log.SaveTextToLog(typErrStr.FN, "GetPeriodReport", strFN, typErrStr.errStr);
			return false;
		}
		string strFN2 = "<InputParameters><Parameters FN='" + typErrStr.FN + "'/></InputParameters>";
		WebCheck1.ClassFiscal w = All.W1;
		if (Operators.CompareString(w.StatusFN(), "", false) == 0 && w.Initialization(strFN2))
		{
			bool periodReport = w.GetPeriodReport(strFN);
			string repXML = "";
			if (periodReport)
			{
				repXML = w.CheckLine.CheckArrayToXML();
			}
			All.ReplyRemember(typErrStr.FN, w.StatusBarXML(), repXML);
			w.Finalization(strFN2);
			return periodReport;
		}
		w = null;
		WebCheck2.ClassFiscal w2 = All.W2;
		if (Operators.CompareString(w2.StatusFN(), "", false) == 0 && w2.Initialization(strFN2))
		{
			bool periodReport2 = w2.GetPeriodReport(strFN);
			string repXML2 = "";
			if (periodReport2)
			{
				repXML2 = w2.CheckLine.CheckArrayToXML();
			}
			All.ReplyRemember(typErrStr.FN, w2.StatusBarXML(), repXML2);
			w2.Finalization(strFN2);
			return periodReport2;
		}
		w2 = null;
		WebCheck3.ClassFiscal w3 = All.W3;
		if (Operators.CompareString(w3.StatusFN(), "", false) == 0 && w3.Initialization(strFN2))
		{
			bool periodReport3 = w3.GetPeriodReport(strFN);
			string repXML3 = "";
			if (periodReport3)
			{
				repXML3 = w3.CheckLine.CheckArrayToXML();
			}
			All.ReplyRemember(typErrStr.FN, w3.StatusBarXML(), repXML3);
			w3.Finalization(strFN2);
			return periodReport3;
		}
		w3 = null;
		WebCheck4.ClassFiscal w4 = All.W4;
		if (Operators.CompareString(w4.StatusFN(), "", false) == 0 && w4.Initialization(strFN2))
		{
			bool periodReport4 = w4.GetPeriodReport(strFN);
			string repXML4 = "";
			if (periodReport4)
			{
				repXML4 = w4.CheckLine.CheckArrayToXML();
			}
			All.ReplyRemember(typErrStr.FN, w4.StatusBarXML(), repXML4);
			w4.Finalization(strFN2);
			return periodReport4;
		}
		w4 = null;
		WebCheck5.ClassFiscal w5 = All.W5;
		if (Operators.CompareString(w5.StatusFN(), "", false) == 0 && w5.Initialization(strFN2))
		{
			bool periodReport5 = w5.GetPeriodReport(strFN);
			string repXML5 = "";
			if (periodReport5)
			{
				repXML5 = w5.CheckLine.CheckArrayToXML();
			}
			All.ReplyRemember(typErrStr.FN, w5.StatusBarXML(), repXML5);
			w5.Finalization(strFN2);
			return periodReport5;
		}
		w5 = null;
		WebCheck6.ClassFiscal w6 = All.W6;
		if (Operators.CompareString(w6.StatusFN(), "", false) == 0 && w6.Initialization(strFN2))
		{
			bool periodReport6 = w6.GetPeriodReport(strFN);
			string repXML6 = "";
			if (periodReport6)
			{
				repXML6 = w6.CheckLine.CheckArrayToXML();
			}
			All.ReplyRemember(typErrStr.FN, w6.StatusBarXML(), repXML6);
			w6.Finalization(strFN2);
			return periodReport6;
		}
		w6 = null;
		WebCheck7.ClassFiscal w7 = All.W7;
		if (Operators.CompareString(w7.StatusFN(), "", false) == 0 && w7.Initialization(strFN2))
		{
			bool periodReport7 = w7.GetPeriodReport(strFN);
			string repXML7 = "";
			if (periodReport7)
			{
				repXML7 = w7.CheckLine.CheckArrayToXML();
			}
			All.ReplyRemember(typErrStr.FN, w7.StatusBarXML(), repXML7);
			w7.Finalization(strFN2);
			return periodReport7;
		}
		w7 = null;
		WebCheck8.ClassFiscal w8 = All.W8;
		if (Operators.CompareString(w8.StatusFN(), "", false) == 0 && w8.Initialization(strFN2))
		{
			bool periodReport8 = w8.GetPeriodReport(strFN);
			string repXML8 = "";
			if (periodReport8)
			{
				repXML8 = w8.CheckLine.CheckArrayToXML();
			}
			All.ReplyRemember(typErrStr.FN, w8.StatusBarXML(), repXML8);
			w8.Finalization(strFN2);
			return periodReport8;
		}
		w8 = null;
		WebCheck9.ClassFiscal w9 = All.W9;
		if (Operators.CompareString(w9.StatusFN(), "", false) == 0 && w9.Initialization(strFN2))
		{
			bool periodReport9 = w9.GetPeriodReport(strFN);
			string repXML9 = "";
			if (periodReport9)
			{
				repXML9 = w9.CheckLine.CheckArrayToXML();
			}
			All.ReplyRemember(typErrStr.FN, w9.StatusBarXML(), repXML9);
			w9.Finalization(strFN2);
			return periodReport9;
		}
		w9 = null;
		WebCheck10.ClassFiscal w10 = All.W10;
		if (Operators.CompareString(w10.StatusFN(), "", false) == 0 && w10.Initialization(strFN2))
		{
			bool periodReport10 = w10.GetPeriodReport(strFN);
			string repXML10 = "";
			if (periodReport10)
			{
				repXML10 = w10.CheckLine.CheckArrayToXML();
			}
			All.ReplyRemember(typErrStr.FN, w10.StatusBarXML(), repXML10);
			w10.Finalization(strFN2);
			return periodReport10;
		}
		w10 = null;
		WebCheck11.ClassFiscal w11 = All.W11;
		if (Operators.CompareString(w11.StatusFN(), "", false) == 0 && w11.Initialization(strFN2))
		{
			bool periodReport11 = w11.GetPeriodReport(strFN);
			string repXML11 = "";
			if (periodReport11)
			{
				repXML11 = w11.CheckLine.CheckArrayToXML();
			}
			All.ReplyRemember(typErrStr.FN, w11.StatusBarXML(), repXML11);
			w11.Finalization(strFN2);
			return periodReport11;
		}
		w11 = null;
		WebCheck12.ClassFiscal w12 = All.W12;
		if (Operators.CompareString(w12.StatusFN(), "", false) == 0 && w12.Initialization(strFN2))
		{
			bool periodReport12 = w12.GetPeriodReport(strFN);
			string repXML12 = "";
			if (periodReport12)
			{
				repXML12 = w12.CheckLine.CheckArrayToXML();
			}
			All.ReplyRemember(typErrStr.FN, w12.StatusBarXML(), repXML12);
			w12.Finalization(strFN2);
			return periodReport12;
		}
		w12 = null;
		WebCheck13.ClassFiscal w13 = All.W13;
		if (Operators.CompareString(w13.StatusFN(), "", false) == 0 && w13.Initialization(strFN2))
		{
			bool periodReport13 = w13.GetPeriodReport(strFN);
			string repXML13 = "";
			if (periodReport13)
			{
				repXML13 = w13.CheckLine.CheckArrayToXML();
			}
			All.ReplyRemember(typErrStr.FN, w13.StatusBarXML(), repXML13);
			w13.Finalization(strFN2);
			return periodReport13;
		}
		w13 = null;
		WebCheck14.ClassFiscal w14 = All.W14;
		if (Operators.CompareString(w14.StatusFN(), "", false) == 0 && w14.Initialization(strFN2))
		{
			bool periodReport14 = w14.GetPeriodReport(strFN);
			string repXML14 = "";
			if (periodReport14)
			{
				repXML14 = w14.CheckLine.CheckArrayToXML();
			}
			All.ReplyRemember(typErrStr.FN, w14.StatusBarXML(), repXML14);
			w14.Finalization(strFN2);
			return periodReport14;
		}
		w14 = null;
		WebCheck15.ClassFiscal w15 = All.W15;
		if (Operators.CompareString(w15.StatusFN(), "", false) == 0 && w15.Initialization(strFN2))
		{
			bool periodReport15 = w15.GetPeriodReport(strFN);
			string repXML15 = "";
			if (periodReport15)
			{
				repXML15 = w15.CheckLine.CheckArrayToXML();
			}
			All.ReplyRemember(typErrStr.FN, w15.StatusBarXML(), repXML15);
			w15.Finalization(strFN2);
			return periodReport15;
		}
		w15 = null;
		WebCheck16.ClassFiscal w16 = All.W16;
		if (Operators.CompareString(w16.StatusFN(), "", false) == 0 && w16.Initialization(strFN2))
		{
			bool periodReport16 = w16.GetPeriodReport(strFN);
			string repXML16 = "";
			if (periodReport16)
			{
				repXML16 = w16.CheckLine.CheckArrayToXML();
			}
			All.ReplyRemember(typErrStr.FN, w16.StatusBarXML(), repXML16);
			w16.Finalization(strFN2);
			return periodReport16;
		}
		w16 = null;
		WebCheck17.ClassFiscal w17 = All.W17;
		if (Operators.CompareString(w17.StatusFN(), "", false) == 0 && w17.Initialization(strFN2))
		{
			bool periodReport17 = w17.GetPeriodReport(strFN);
			string repXML17 = "";
			if (periodReport17)
			{
				repXML17 = w17.CheckLine.CheckArrayToXML();
			}
			All.ReplyRemember(typErrStr.FN, w17.StatusBarXML(), repXML17);
			w17.Finalization(strFN2);
			return periodReport17;
		}
		w17 = null;
		WebCheck18.ClassFiscal w18 = All.W18;
		if (Operators.CompareString(w18.StatusFN(), "", false) == 0 && w18.Initialization(strFN2))
		{
			bool periodReport18 = w18.GetPeriodReport(strFN);
			string repXML18 = "";
			if (periodReport18)
			{
				repXML18 = w18.CheckLine.CheckArrayToXML();
			}
			All.ReplyRemember(typErrStr.FN, w18.StatusBarXML(), repXML18);
			w18.Finalization(strFN2);
			return periodReport18;
		}
		w18 = null;
		All.Log.SaveTextToLog(typErrStr.FN, "GetPeriodReport", strFN, "Все слоты заняты транзакциями с сервером налоговой");
		All.ReplyRemember(typErrStr.FN, All.XMLRiplyErr(typErrStr.FN, "Все слоты заняты транзакциями с сервером налоговой"));
		return false;
	}

	public bool GetCheckFNbyUID(string strFN)
	{
		TypErrStr typErrStr = All.TestFN(strFN);
		if (typErrStr.errCode > 0)
		{
			All.ReplyRemember(typErrStr.FN, All.XMLRiplyErr(typErrStr.FN, typErrStr.errStr));
			All.Log.SaveTextToLog(typErrStr.FN, "GetCheckFNbyUID", strFN, typErrStr.errStr);
			return false;
		}
		string strFN2 = "<InputParameters><Parameters FN='" + typErrStr.FN + "'/></InputParameters>";
		WebCheck1.ClassFiscal w = All.W1;
		if (Operators.CompareString(w.StatusFN(), "", false) == 0 && w.Initialization(strFN2))
		{
			bool checkFNbyUID = w.GetCheckFNbyUID(strFN);
			All.ReplyRemember(typErrStr.FN, w.StatusBarXML());
			w.Finalization(strFN2);
			return checkFNbyUID;
		}
		w = null;
		WebCheck2.ClassFiscal w2 = All.W2;
		if (Operators.CompareString(w2.StatusFN(), "", false) == 0 && w2.Initialization(strFN2))
		{
			bool checkFNbyUID2 = w2.GetCheckFNbyUID(strFN);
			All.ReplyRemember(typErrStr.FN, w2.StatusBarXML());
			w2.Finalization(strFN2);
			return checkFNbyUID2;
		}
		w2 = null;
		WebCheck3.ClassFiscal w3 = All.W3;
		if (Operators.CompareString(w3.StatusFN(), "", false) == 0 && w3.Initialization(strFN2))
		{
			bool checkFNbyUID3 = w3.GetCheckFNbyUID(strFN);
			All.ReplyRemember(typErrStr.FN, w3.StatusBarXML());
			w3.Finalization(strFN2);
			return checkFNbyUID3;
		}
		w3 = null;
		WebCheck4.ClassFiscal w4 = All.W4;
		if (Operators.CompareString(w4.StatusFN(), "", false) == 0 && w4.Initialization(strFN2))
		{
			bool checkFNbyUID4 = w4.GetCheckFNbyUID(strFN);
			All.ReplyRemember(typErrStr.FN, w4.StatusBarXML());
			w4.Finalization(strFN2);
			return checkFNbyUID4;
		}
		w4 = null;
		WebCheck5.ClassFiscal w5 = All.W5;
		if (Operators.CompareString(w5.StatusFN(), "", false) == 0 && w5.Initialization(strFN2))
		{
			bool checkFNbyUID5 = w5.GetCheckFNbyUID(strFN);
			All.ReplyRemember(typErrStr.FN, w5.StatusBarXML());
			w5.Finalization(strFN2);
			return checkFNbyUID5;
		}
		w5 = null;
		WebCheck6.ClassFiscal w6 = All.W6;
		if (Operators.CompareString(w6.StatusFN(), "", false) == 0 && w6.Initialization(strFN2))
		{
			bool checkFNbyUID6 = w6.GetCheckFNbyUID(strFN);
			All.ReplyRemember(typErrStr.FN, w6.StatusBarXML());
			w6.Finalization(strFN2);
			return checkFNbyUID6;
		}
		w6 = null;
		WebCheck7.ClassFiscal w7 = All.W7;
		if (Operators.CompareString(w7.StatusFN(), "", false) == 0 && w7.Initialization(strFN2))
		{
			bool checkFNbyUID7 = w7.GetCheckFNbyUID(strFN);
			All.ReplyRemember(typErrStr.FN, w7.StatusBarXML());
			w7.Finalization(strFN2);
			return checkFNbyUID7;
		}
		w7 = null;
		WebCheck8.ClassFiscal w8 = All.W8;
		if (Operators.CompareString(w8.StatusFN(), "", false) == 0 && w8.Initialization(strFN2))
		{
			bool checkFNbyUID8 = w8.GetCheckFNbyUID(strFN);
			All.ReplyRemember(typErrStr.FN, w8.StatusBarXML());
			w8.Finalization(strFN2);
			return checkFNbyUID8;
		}
		w8 = null;
		WebCheck9.ClassFiscal w9 = All.W9;
		if (Operators.CompareString(w9.StatusFN(), "", false) == 0 && w9.Initialization(strFN2))
		{
			bool checkFNbyUID9 = w9.GetCheckFNbyUID(strFN);
			All.ReplyRemember(typErrStr.FN, w9.StatusBarXML());
			w9.Finalization(strFN2);
			return checkFNbyUID9;
		}
		w9 = null;
		WebCheck10.ClassFiscal w10 = All.W10;
		if (Operators.CompareString(w10.StatusFN(), "", false) == 0 && w10.Initialization(strFN2))
		{
			bool checkFNbyUID10 = w10.GetCheckFNbyUID(strFN);
			All.ReplyRemember(typErrStr.FN, w10.StatusBarXML());
			w10.Finalization(strFN2);
			return checkFNbyUID10;
		}
		w10 = null;
		WebCheck11.ClassFiscal w11 = All.W11;
		if (Operators.CompareString(w11.StatusFN(), "", false) == 0 && w11.Initialization(strFN2))
		{
			bool checkFNbyUID11 = w11.GetCheckFNbyUID(strFN);
			All.ReplyRemember(typErrStr.FN, w11.StatusBarXML());
			w11.Finalization(strFN2);
			return checkFNbyUID11;
		}
		w11 = null;
		WebCheck12.ClassFiscal w12 = All.W12;
		if (Operators.CompareString(w12.StatusFN(), "", false) == 0 && w12.Initialization(strFN2))
		{
			bool checkFNbyUID12 = w12.GetCheckFNbyUID(strFN);
			All.ReplyRemember(typErrStr.FN, w12.StatusBarXML());
			w12.Finalization(strFN2);
			return checkFNbyUID12;
		}
		w12 = null;
		WebCheck13.ClassFiscal w13 = All.W13;
		if (Operators.CompareString(w13.StatusFN(), "", false) == 0 && w13.Initialization(strFN2))
		{
			bool checkFNbyUID13 = w13.GetCheckFNbyUID(strFN);
			All.ReplyRemember(typErrStr.FN, w13.StatusBarXML());
			w13.Finalization(strFN2);
			return checkFNbyUID13;
		}
		w13 = null;
		WebCheck14.ClassFiscal w14 = All.W14;
		if (Operators.CompareString(w14.StatusFN(), "", false) == 0 && w14.Initialization(strFN2))
		{
			bool checkFNbyUID14 = w14.GetCheckFNbyUID(strFN);
			All.ReplyRemember(typErrStr.FN, w14.StatusBarXML());
			w14.Finalization(strFN2);
			return checkFNbyUID14;
		}
		w14 = null;
		WebCheck15.ClassFiscal w15 = All.W15;
		if (Operators.CompareString(w15.StatusFN(), "", false) == 0 && w15.Initialization(strFN2))
		{
			bool checkFNbyUID15 = w15.GetCheckFNbyUID(strFN);
			All.ReplyRemember(typErrStr.FN, w15.StatusBarXML());
			w15.Finalization(strFN2);
			return checkFNbyUID15;
		}
		w15 = null;
		WebCheck16.ClassFiscal w16 = All.W16;
		if (Operators.CompareString(w16.StatusFN(), "", false) == 0 && w16.Initialization(strFN2))
		{
			bool checkFNbyUID16 = w16.GetCheckFNbyUID(strFN);
			All.ReplyRemember(typErrStr.FN, w16.StatusBarXML());
			w16.Finalization(strFN2);
			return checkFNbyUID16;
		}
		w16 = null;
		WebCheck17.ClassFiscal w17 = All.W17;
		if (Operators.CompareString(w17.StatusFN(), "", false) == 0 && w17.Initialization(strFN2))
		{
			bool checkFNbyUID17 = w17.GetCheckFNbyUID(strFN);
			All.ReplyRemember(typErrStr.FN, w17.StatusBarXML());
			w17.Finalization(strFN2);
			return checkFNbyUID17;
		}
		w17 = null;
		WebCheck18.ClassFiscal w18 = All.W18;
		if (Operators.CompareString(w18.StatusFN(), "", false) == 0 && w18.Initialization(strFN2))
		{
			bool checkFNbyUID18 = w18.GetCheckFNbyUID(strFN);
			All.ReplyRemember(typErrStr.FN, w18.StatusBarXML());
			w18.Finalization(strFN2);
			return checkFNbyUID18;
		}
		w18 = null;
		All.Log.SaveTextToLog(typErrStr.FN, "GetCheckFNbyUID", strFN, "Все слоты заняты транзакциями с сервером налоговой");
		All.ReplyRemember(typErrStr.FN, All.XMLRiplyErr(typErrStr.FN, "Все слоты заняты транзакциями с сервером налоговой"));
		return false;
	}

	public bool OfflineToOnline(string strFN)
	{
		return false;
	}

	public bool GetOfflineNumbers(string strFN)
	{
		return false;
	}

	public bool OnlineToOffline(string strFN)
	{
		TypErrStr typErrStr = All.TestFN(strFN);
		if (typErrStr.errCode > 0)
		{
			All.ReplyRemember(typErrStr.FN, All.XMLRiplyErr(typErrStr.FN, typErrStr.errStr));
			All.Log.SaveTextToLog(typErrStr.FN, "OnlineToOffline", strFN, typErrStr.errStr);
			return false;
		}
		string strFN2 = "<InputParameters><Parameters FN='" + typErrStr.FN + "'/></InputParameters>";
		WebCheck1.ClassFiscal w = All.W1;
		if (Operators.CompareString(w.StatusFN(), "", false) == 0 && w.Initialization(strFN2))
		{
			bool num = w.OnlineToOffline(strFN);
			string repXML = "";
			if (num)
			{
				repXML = w.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w.StatusBarXML(), repXML);
			w.Finalization(strFN2);
			return num;
		}
		w = null;
		WebCheck2.ClassFiscal w2 = All.W2;
		if (Operators.CompareString(w2.StatusFN(), "", false) == 0 && w2.Initialization(strFN2))
		{
			bool num2 = w2.OnlineToOffline(strFN);
			string repXML2 = "";
			if (num2)
			{
				repXML2 = w2.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w2.StatusBarXML(), repXML2);
			w2.Finalization(strFN2);
			return num2;
		}
		w2 = null;
		WebCheck3.ClassFiscal w3 = All.W3;
		if (Operators.CompareString(w3.StatusFN(), "", false) == 0 && w3.Initialization(strFN2))
		{
			bool num3 = w3.OnlineToOffline(strFN);
			string repXML3 = "";
			if (num3)
			{
				repXML3 = w3.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w3.StatusBarXML(), repXML3);
			w3.Finalization(strFN2);
			return num3;
		}
		w3 = null;
		WebCheck4.ClassFiscal w4 = All.W4;
		if (Operators.CompareString(w4.StatusFN(), "", false) == 0 && w4.Initialization(strFN2))
		{
			bool num4 = w4.OnlineToOffline(strFN);
			string repXML4 = "";
			if (num4)
			{
				repXML4 = w4.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w4.StatusBarXML(), repXML4);
			w4.Finalization(strFN2);
			return num4;
		}
		w4 = null;
		WebCheck5.ClassFiscal w5 = All.W5;
		if (Operators.CompareString(w5.StatusFN(), "", false) == 0 && w5.Initialization(strFN2))
		{
			bool num5 = w5.OnlineToOffline(strFN);
			string repXML5 = "";
			if (num5)
			{
				repXML5 = w5.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w5.StatusBarXML(), repXML5);
			w5.Finalization(strFN2);
			return num5;
		}
		w5 = null;
		WebCheck6.ClassFiscal w6 = All.W6;
		if (Operators.CompareString(w6.StatusFN(), "", false) == 0 && w6.Initialization(strFN2))
		{
			bool num6 = w6.OnlineToOffline(strFN);
			string repXML6 = "";
			if (num6)
			{
				repXML6 = w6.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w6.StatusBarXML(), repXML6);
			w6.Finalization(strFN2);
			return num6;
		}
		w6 = null;
		WebCheck7.ClassFiscal w7 = All.W7;
		if (Operators.CompareString(w7.StatusFN(), "", false) == 0 && w7.Initialization(strFN2))
		{
			bool num7 = w7.OnlineToOffline(strFN);
			string repXML7 = "";
			if (num7)
			{
				repXML7 = w7.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w7.StatusBarXML(), repXML7);
			w7.Finalization(strFN2);
			return num7;
		}
		w7 = null;
		WebCheck8.ClassFiscal w8 = All.W8;
		if (Operators.CompareString(w8.StatusFN(), "", false) == 0 && w8.Initialization(strFN2))
		{
			bool num8 = w8.OnlineToOffline(strFN);
			string repXML8 = "";
			if (num8)
			{
				repXML8 = w8.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w8.StatusBarXML(), repXML8);
			w8.Finalization(strFN2);
			return num8;
		}
		w8 = null;
		WebCheck9.ClassFiscal w9 = All.W9;
		if (Operators.CompareString(w9.StatusFN(), "", false) == 0 && w9.Initialization(strFN2))
		{
			bool num9 = w9.OnlineToOffline(strFN);
			string repXML9 = "";
			if (num9)
			{
				repXML9 = w9.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w9.StatusBarXML(), repXML9);
			w9.Finalization(strFN2);
			return num9;
		}
		w9 = null;
		WebCheck10.ClassFiscal w10 = All.W10;
		if (Operators.CompareString(w10.StatusFN(), "", false) == 0 && w10.Initialization(strFN2))
		{
			bool num10 = w10.OnlineToOffline(strFN);
			string repXML10 = "";
			if (num10)
			{
				repXML10 = w10.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w10.StatusBarXML(), repXML10);
			w10.Finalization(strFN2);
			return num10;
		}
		w10 = null;
		WebCheck11.ClassFiscal w11 = All.W11;
		if (Operators.CompareString(w11.StatusFN(), "", false) == 0 && w11.Initialization(strFN2))
		{
			bool num11 = w11.OnlineToOffline(strFN);
			string repXML11 = "";
			if (num11)
			{
				repXML11 = w11.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w11.StatusBarXML(), repXML11);
			w11.Finalization(strFN2);
			return num11;
		}
		w11 = null;
		WebCheck12.ClassFiscal w12 = All.W12;
		if (Operators.CompareString(w12.StatusFN(), "", false) == 0 && w12.Initialization(strFN2))
		{
			bool num12 = w12.OnlineToOffline(strFN);
			string repXML12 = "";
			if (num12)
			{
				repXML12 = w12.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w12.StatusBarXML(), repXML12);
			w12.Finalization(strFN2);
			return num12;
		}
		w12 = null;
		WebCheck13.ClassFiscal w13 = All.W13;
		if (Operators.CompareString(w13.StatusFN(), "", false) == 0 && w13.Initialization(strFN2))
		{
			bool num13 = w13.OnlineToOffline(strFN);
			string repXML13 = "";
			if (num13)
			{
				repXML13 = w13.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w13.StatusBarXML(), repXML13);
			w13.Finalization(strFN2);
			return num13;
		}
		w13 = null;
		WebCheck14.ClassFiscal w14 = All.W14;
		if (Operators.CompareString(w14.StatusFN(), "", false) == 0 && w14.Initialization(strFN2))
		{
			bool num14 = w14.OnlineToOffline(strFN);
			string repXML14 = "";
			if (num14)
			{
				repXML14 = w14.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w14.StatusBarXML(), repXML14);
			w14.Finalization(strFN2);
			return num14;
		}
		w14 = null;
		WebCheck15.ClassFiscal w15 = All.W15;
		if (Operators.CompareString(w15.StatusFN(), "", false) == 0 && w15.Initialization(strFN2))
		{
			bool num15 = w15.OnlineToOffline(strFN);
			string repXML15 = "";
			if (num15)
			{
				repXML15 = w15.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w15.StatusBarXML(), repXML15);
			w15.Finalization(strFN2);
			return num15;
		}
		w15 = null;
		WebCheck16.ClassFiscal w16 = All.W16;
		if (Operators.CompareString(w16.StatusFN(), "", false) == 0 && w16.Initialization(strFN2))
		{
			bool num16 = w16.OnlineToOffline(strFN);
			string repXML16 = "";
			if (num16)
			{
				repXML16 = w16.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w16.StatusBarXML(), repXML16);
			w16.Finalization(strFN2);
			return num16;
		}
		w16 = null;
		WebCheck17.ClassFiscal w17 = All.W17;
		if (Operators.CompareString(w17.StatusFN(), "", false) == 0 && w17.Initialization(strFN2))
		{
			bool num17 = w17.OnlineToOffline(strFN);
			string repXML17 = "";
			if (num17)
			{
				repXML17 = w17.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w17.StatusBarXML(), repXML17);
			w17.Finalization(strFN2);
			return num17;
		}
		w17 = null;
		WebCheck18.ClassFiscal w18 = All.W18;
		if (Operators.CompareString(w18.StatusFN(), "", false) == 0 && w18.Initialization(strFN2))
		{
			bool num18 = w18.OnlineToOffline(strFN);
			string repXML18 = "";
			if (num18)
			{
				repXML18 = w18.CheckLine.CheckXML("");
			}
			All.ReplyRemember(typErrStr.FN, w18.StatusBarXML(), repXML18);
			w18.Finalization(strFN2);
			return num18;
		}
		w18 = null;
		All.Log.SaveTextToLog(typErrStr.FN, "OnlineToOffline", strFN, "Все слоты заняты транзакциями с сервером налоговой");
		All.ReplyRemember(typErrStr.FN, All.XMLRiplyErr(typErrStr.FN, "Все слоты заняты транзакциями с сервером налоговой"));
		return false;
	}

	public bool SetSignSettings(string strFN)
	{
		TypErrStr parametrToString = All.GetParametrToString(strFN, "keypath");
		parametrToString = All.GetParametrToString(strFN, "keypass", "InputParameters/Parameters", RegUpLow: true);
		if (parametrToString.errCode > 0)
		{
			parametrToString = All.GetParametrToString(strFN, "KeyPass", "InputParameters/Parameters", RegUpLow: true);
		}
		if (parametrToString.errCode > 0)
		{
			parametrToString = All.GetParametrToString(strFN, "KEYPASS", "InputParameters/Parameters", RegUpLow: true);
		}
		if (parametrToString.errCode > 0)
		{
			parametrToString = All.GetParametrToString(strFN, "Keypass", "InputParameters/Parameters", RegUpLow: true);
		}
		if (parametrToString.errCode > 0)
		{
			parametrToString = All.GetParametrToString(strFN, "keyPass", "InputParameters/Parameters", RegUpLow: true);
		}
		return true;
	}
}
